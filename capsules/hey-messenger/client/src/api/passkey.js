// Hey Messenger passkey auth.
//
// Two sign-in paths, exposed for the SignInGate UI:
//
//   signInViaRuntime()        — primary. Calls upstream's
//                               /api/auth/passkey/authenticate/{begin,complete}
//                               with the cross-capsule unified-identity PRF
//                               extension. Derives Ed25519 + X25519 + ML-KEM
//                               keys from the PRF output. Same passkey across
//                               all Elastos capsules → same DID.
//
//   signInWithGeneratedKey()  — fallback for users without a passkey-capable
//                               authenticator. Generates a random 32-byte
//                               recovery key; user must save it.

import { startAuthentication } from "@simplewebauthn/browser";
import { apiUrl, bearerReady } from "../lib/runtime.js";
import {
  generateAuthKey,
  bytesToHex,
  ELASTOS_IDENTITY_PRF_INPUT,
} from "../lib/identity.js";
import { setSession, getDidKey } from "../lib/session.js";

const ADOPTED_IDENTITY_LS = "hey-messenger-adopted-identity";

const b64u = {
  decode: (b64uStr) => {
    const pad = (4 - (b64uStr.length % 4)) % 4;
    const b64 = b64uStr.replace(/-/g, "+").replace(/_/g, "/") + "=".repeat(pad);
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  },
};

// SimpleWebAuthn returns the PRF output base64url-encoded; raw WebAuthn
// surfaces it as ArrayBuffer. Accept either, return Uint8Array(32) or null.
const decodePrfValue = (v) => {
  if (!v) return null;
  try {
    const bytes = typeof v === "string" ? b64u.decode(v) : new Uint8Array(v);
    return bytes.length === 32 ? bytes : null;
  } catch { return null; }
};
const prfIdentityFromResponse = (resp) =>
  decodePrfValue(resp?.clientExtensionResults?.prf?.results?.first);

const upstreamFetch = async (path, init = {}) => {
  await bearerReady.catch(() => false);
  return fetch(apiUrl(path), {
    method: init.method || "GET",
    credentials: "include",
    headers: { "Content-Type": "application/json", ...(init.headers || {}) },
    body: init.body,
  });
};

const rememberAdoption = (didKey, name, source) => {
  try {
    localStorage.setItem(
      ADOPTED_IDENTITY_LS,
      JSON.stringify({
        didKey,
        name: name || "You",
        source,
        adoptedAt: new Date().toISOString(),
      }),
    );
  } catch (_) {}
};

// Sign in to the runtime via the user's existing passkey (the same one
// they registered in System). Returns { didKey, name, source }.
//
// Throws with a friendly message on:
//   - upstream has no credentials (user never finished System signup)
//   - user cancels the passkey prompt
//   - authenticator lacks PRF (can't derive signing key)
export const signInViaRuntime = async (nickname = null) => {
  // 1. Get options from upstream.
  let beginResp = await upstreamFetch("/api/auth/passkey/authenticate/begin", {
    method: "POST",
    body: "{}",
  });
  if (beginResp.status === 405) {
    beginResp = await upstreamFetch("/api/auth/passkey/authenticate/begin", { method: "GET" });
  }
  if (!beginResp.ok) {
    if (beginResp.status === 400 || beginResp.status === 404) {
      throw new Error(
        "No passkey set up on this device yet. Go back to System and create a passkey first, then come back here.",
      );
    }
    throw new Error(`passkey authenticate/begin: HTTP ${beginResp.status}`);
  }
  const beginJson = await beginResp.json();
  console.info("[hey-msg-signin] /authenticate/begin response:", beginJson);
  // Upstream v0.3 shape:
  //   { schema, ceremony_id, options: { publicKey: <WebAuthn options> } }
  const ceremonyId = beginJson?.ceremony_id || beginJson?.ceremonyId || null;
  const options =
    beginJson?.options?.publicKey ||
    beginJson?.publicKey ||
    beginJson?.options ||
    beginJson;
  if (!options?.challenge) {
    throw new Error(
      "passkey authenticate/begin response is missing a 'challenge' field — see [hey-msg-signin] log above for the raw response.",
    );
  }

  // 2. Inject the cross-capsule PRF extension.
  options.extensions = options.extensions || {};
  options.extensions.prf = options.extensions.prf || {
    eval: { first: ELASTOS_IDENTITY_PRF_INPUT },
  };

  // 3. Run the WebAuthn ceremony.
  const assertion = await startAuthentication({ optionsJSON: options });

  // 4. POST the assertion to upstream — full upstream contract
  // (reverse-engineered for hey-social; same applies here):
  //   outer:          { ceremony_id, response }
  //   assertion:      { id, rawId, response, type }    — no clientExtensionResults
  //   inner response: { clientDataJson, authenticatorData, signature, userHandle }
  //                    — clientDataJson lowercase j
  // PRF for signing-key derivation is read browser-side BEFORE we POST,
  // so removing clientExtensionResults from the payload is harmless.
  const normalizedAssertion = {
    id: assertion.id,
    rawId: assertion.rawId,
    type: assertion.type || "public-key",
    response: {
      clientDataJson: assertion.response?.clientDataJSON,
      authenticatorData: assertion.response?.authenticatorData,
      signature: assertion.response?.signature,
      userHandle: assertion.response?.userHandle ?? null,
    },
  };
  const completeBody = ceremonyId
    ? { ceremony_id: ceremonyId, response: normalizedAssertion }
    : normalizedAssertion;
  const completeResp = await upstreamFetch("/api/auth/passkey/authenticate/complete", {
    method: "POST",
    body: JSON.stringify(completeBody),
  });
  if (!completeResp.ok) {
    const txt = await completeResp.text().catch(() => "");
    throw new Error(`passkey authenticate/complete: HTTP ${completeResp.status} ${txt.slice(0, 200)}`);
  }
  let upstreamResult = null;
  try { upstreamResult = await completeResp.json(); } catch (_) {}

  // 5. Derive the messenger's signing identity from PRF output.
  const identityPrf = prfIdentityFromResponse(assertion);
  if (!identityPrf) {
    throw new Error(
      "Passkey didn't return PRF output — your authenticator lacks the prf extension. " +
      "Use a PRF-capable passkey (Yubikey 5.7+, Touch ID macOS 14+, modern Windows Hello, Android 14+).",
    );
  }
  const authKey = bytesToHex(identityPrf);
  await setSession(authKey);
  const didKey = getDidKey();

  // 6. Cache the adopted identity for the UI.
  const upstreamDisplayName =
    upstreamResult?.displayName ||
    upstreamResult?.name ||
    upstreamResult?.user?.name ||
    upstreamResult?.user?.displayName ||
    null;
  const name = (nickname && nickname.trim()) || upstreamDisplayName || "You";
  rememberAdoption(didKey, name, "runtime-passkey");

  // Mirror the identity into the cross-capsule shared file so Hey
  // Social (and any future Hey capsule) sees the same user without
  // needing its own passkey ceremony. Writes both canonical and
  // legacy paths; fire-and-forget — non-fatal on failure.
  try {
    const { sharedStorage } = await import("../lib/runtime.js");
    const profile = {
      name,
      didKey,
      recoveryKeyHash: "",
      passkeys: [],
      createdAt: new Date().toISOString(),
      createdBy: "hey-messenger-runtime-signin",
    };
    sharedStorage.writeJson(".AppData/ElastOS/Identity/profile.json", profile).catch(() => {});
    sharedStorage.writeJson(".AppData/Identity/profile.json", profile).catch(() => {});
  } catch (err) {
    console.warn("[hey-messenger] writeSharedIdentity failed at signin", err);
  }

  return { didKey, name, source: "runtime-passkey" };
};

// Recovery-key fallback. Generates a random 32-byte authKey, sets the
// session, caches the adopted identity. Returns { authKey, didKey, name }
// so the caller can SHOW the authKey to the user once.
export const signInWithGeneratedKey = async ({ name }) => {
  const authKey = generateAuthKey();
  await setSession(authKey);
  const didKey = getDidKey();
  const display = (name || "").trim() || "You";
  rememberAdoption(didKey, display, "recovery-key-generated");
  return { authKey, didKey, name: display };
};

// Same shape as Hey Social's passkeySupported helper.
export const passkeySupported = () => {
  if (typeof window === "undefined") return false;
  try {
    return !!window.PublicKeyCredential;
  } catch { return false; }
};
