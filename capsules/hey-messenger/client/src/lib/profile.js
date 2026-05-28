// Profile bundles — how peers learn each other's encryption pubkeys.
//
// Each user publishes a signed "profile.bundle" event to their own
// Carrier topic on sign-in. Anyone wanting to E2E-encrypt to them
// subscribes to that topic, fetches the latest bundle, verifies the
// signature, and caches it locally.
//
// Topic: hey-msg/v0/profile/<did>
// Event payload shape:
//   { name?, x25519Pub: <hex>, kemPub: <hex> }
//
// The event is signed with the user's Ed25519 key (createSignedEvent),
// so the binding "this DID publishes these pubkeys" is cryptographic.
// Cached bundles live in localStorage so we don't re-fetch on every send.

import { peer } from "./runtime.js";
import { createSignedEvent, verifySignedEvent } from "./events.js";
import { getKeypair, getDidKey } from "./session.js";

const BUNDLE_CACHE_LS = "hey-messenger-profile-bundles";

const profileTopic = (did) => `hey-msg/v0/profile/${did}`;

const hexToBytes = (hex) => {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
};

const bytesToHex = (bytes) => {
  let hex = "";
  for (let i = 0; i < bytes.length; i++) hex += bytes[i].toString(16).padStart(2, "0");
  return hex;
};

const loadBundleCache = () => {
  try { return JSON.parse(localStorage.getItem(BUNDLE_CACHE_LS) || "{}"); }
  catch { return {}; }
};
const saveBundleCache = (m) => {
  try { localStorage.setItem(BUNDLE_CACHE_LS, JSON.stringify(m)); } catch {}
};

// Publish own profile bundle to Carrier. Called once on sign-in and any
// time the user's pubkey set changes (rare — only on key rotation).
export const publishOwnBundle = async ({ name } = {}) => {
  const kp = getKeypair();
  if (!kp) throw new Error("publishOwnBundle: not signed in");
  const payload = {
    name: name || null,
    x25519Pub: bytesToHex(kp.x25519.publicKey),
    kemPub: bytesToHex(kp.kem.publicKey),
  };
  const event = await createSignedEvent({ type: "profile.bundle", payload }, kp);
  await peer.joinTopic(profileTopic(kp.didKey));
  await peer.publish({
    topic: profileTopic(kp.didKey),
    message: JSON.stringify(event),
    sender_id: event.sender_did,
    ts: event.ts,
    signature: event.signature,
  });
  return event;
};

// Resolve a peer's pubkey bundle by DID. Checks the local cache first,
// then subscribes to the peer's profile topic and tries to pull a recent
// bundle event. Returns { x25519Pub: Uint8Array, kemPub: Uint8Array } or
// null if no bundle has been published / received yet.
export const resolveBundle = async (did) => {
  const cache = loadBundleCache();
  if (cache[did]) {
    return {
      x25519Pub: hexToBytes(cache[did].x25519Pub),
      kemPub: hexToBytes(cache[did].kemPub),
      cached: true,
    };
  }

  try {
    await peer.joinTopic(profileTopic(did));
  } catch {
    return null;
  }
  let resp;
  try {
    resp = await peer.recv({
      topic: profileTopic(did),
      limit: 5,
      consumer_id: `hey-messenger:profile:${getDidKey() || "anon"}`,
    });
  } catch {
    return null;
  }
  const items = resp?.data?.messages || resp?.messages || [];
  // Most recent first: scan for a verifiable profile.bundle by this DID.
  for (const item of items) {
    let event;
    try { event = JSON.parse(item.message ?? item); } catch { continue; }
    if (event?.type !== "profile.bundle") continue;
    if (event?.sender_did !== did) continue;
    const v = verifySignedEvent(event);
    if (!v.valid) continue;
    const { x25519Pub, kemPub } = event.payload || {};
    if (typeof x25519Pub !== "string" || typeof kemPub !== "string") continue;
    cache[did] = { x25519Pub, kemPub, at: Date.now() };
    saveBundleCache(cache);
    return {
      x25519Pub: hexToBytes(x25519Pub),
      kemPub: hexToBytes(kemPub),
      cached: false,
    };
  }
  return null;
};

// Forget a cached bundle — useful if a peer rotates keys.
export const forgetBundle = (did) => {
  const cache = loadBundleCache();
  delete cache[did];
  saveBundleCache(cache);
};
