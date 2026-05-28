// Session — persists the user's signing identity across page reloads.
//
// Extracted from Hey's lib/session.js. Differences:
//   - Drops the vault dependency (vault is additive; not wired in Phase 2).
//   - Storage keys rebranded to "hey-messenger-*" to avoid collision when
//     both Hey and Hey Messenger are installed on the same Runtime.
//
// HARDENED: the Ed25519 private key is imported as a NON-EXTRACTABLE
// Web Crypto CryptoKey and held in IndexedDB. The raw recovery seed
// never lives in JS memory or storage after import.
//
// Public surface:
//   getKeypair()    → { didKey, publicKey: Uint8Array, privKey: CryptoKey } | null
//   getDidKey()     → string | null
//   setSession(authKey)   → Promise<void>    sign-in / sign-up
//   clearSession()        → Promise<void>    sign-out (wipes IDB + cache)
//   initSession()         → Promise<void>    boot-time load + legacy migration
//
// The sync getters work because initSession() populates a module-level
// cache before the React tree mounts (main.jsx awaits it).

import { expandKeypair, x25519FromSeed, mlKemFromSeed } from "./identity";
import {
  saveSeedAsSigningKey,
  loadSigningKey,
  deleteSigningKey,
  ed25519Supported as cryptoEd25519Supported,
} from "./keystore";

let cached = null;

const LEGACY_AUTHKEY_LS = "hey-messenger-session";
const PUBKEY_LS = "hey-messenger-public-identity";
const PQ_LS = "hey-messenger-pq-identity"; // X25519 + ML-KEM keys
// HONEST CAVEAT: ML-KEM and X25519 secret keys live in localStorage as
// plain bytes — Web Crypto has no ML-KEM and no X25519-with-non-extractable
// path yet. An XSS attacker on this origin can read them. This is a known
// gap; tracked for Phase 6 (Signal-PQ-ratchet) which moves to per-session
// ephemeral keys and limits the blast radius.

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

const readPubIdentity = () => {
  try {
    const raw = localStorage.getItem(PUBKEY_LS);
    return raw ? JSON.parse(raw) : null;
  } catch { return null; }
};

const writePubIdentity = (data) => {
  try { localStorage.setItem(PUBKEY_LS, JSON.stringify(data)); } catch {}
};

const clearPubIdentity = () => {
  try { localStorage.removeItem(PUBKEY_LS); } catch {}
};

// Derive + cache the PQ + X25519 keypairs from the same seed. Idempotent.
// Persists secret bytes to localStorage (see honest-caveat note above).
const setupPQKeys = (seed) => {
  const x = x25519FromSeed(seed);
  const k = mlKemFromSeed(seed);
  try {
    localStorage.setItem(
      PQ_LS,
      JSON.stringify({
        x25519: { pub: bytesToHex(x.publicKey), priv: bytesToHex(x.privateKey) },
        kem:    { pub: bytesToHex(k.publicKey), priv: bytesToHex(k.secretKey) },
      }),
    );
  } catch {}
  return { x25519: x, kem: k };
};

const loadPQKeys = () => {
  try {
    const raw = localStorage.getItem(PQ_LS);
    if (!raw) return null;
    const j = JSON.parse(raw);
    return {
      x25519: {
        publicKey: hexToBytes(j.x25519.pub),
        privateKey: hexToBytes(j.x25519.priv),
      },
      kem: {
        publicKey: hexToBytes(j.kem.pub),
        secretKey: hexToBytes(j.kem.priv),
      },
    };
  } catch { return null; }
};

const clearPQKeys = () => {
  try { localStorage.removeItem(PQ_LS); } catch {}
};

export const setSession = async (authKey) => {
  if (!authKey) return clearSession();
  const { seed, publicKey, didKey } = expandKeypair(authKey);
  const pq = setupPQKeys(seed);

  let privKey = null;
  if (await cryptoEd25519Supported()) {
    try {
      privKey = await saveSeedAsSigningKey(seed);
    } catch (err) {
      console.warn("[hey-messenger] non-extractable key save failed; falling back", err);
    }
  }

  if (privKey) {
    cached = { didKey, publicKey, privKey, ...pq };
    writePubIdentity({
      didKey,
      pubKeyHex: bytesToHex(publicKey),
      x25519Pub: bytesToHex(pq.x25519.publicKey),
      kemPub: bytesToHex(pq.kem.publicKey),
    });
    try { localStorage.removeItem(LEGACY_AUTHKEY_LS); } catch {}
  } else {
    cached = { didKey, publicKey, seed, privKey: null, _legacy: true, ...pq };
    writePubIdentity({
      didKey,
      pubKeyHex: bytesToHex(publicKey),
      x25519Pub: bytesToHex(pq.x25519.publicKey),
      kemPub: bytesToHex(pq.kem.publicKey),
    });
    try {
      localStorage.setItem(LEGACY_AUTHKEY_LS, JSON.stringify({ authKey }));
    } catch {}
    console.warn(
      "[hey-messenger] hardened key store unavailable — using legacy localStorage seed. " +
      "XSS could exfiltrate the signing key. Update your browser."
    );
  }
};

export const initSession = async () => {
  if (cached) return;

  const privKey = await loadSigningKey().catch(() => null);
  const pubData = readPubIdentity();
  const pq = loadPQKeys();
  if (privKey && pubData && pubData.didKey && pubData.pubKeyHex && pq) {
    cached = {
      didKey: pubData.didKey,
      publicKey: hexToBytes(pubData.pubKeyHex),
      privKey,
      x25519: pq.x25519,
      kem: pq.kem,
    };
    try { localStorage.removeItem(LEGACY_AUTHKEY_LS); } catch {}
    return;
  }

  let legacy = null;
  try {
    const raw = localStorage.getItem(LEGACY_AUTHKEY_LS);
    legacy = raw ? JSON.parse(raw) : null;
  } catch { legacy = null; }
  if (legacy?.authKey) {
    try {
      await setSession(legacy.authKey);
      return;
    } catch (err) {
      console.warn("[hey-messenger] legacy seed migration failed", err);
    }
  }

  cached = null;
};

export const getKeypair = () => cached || null;
export const getDidKey = () => cached?.didKey || null;

export const clearSession = async () => {
  cached = null;
  clearPubIdentity();
  clearPQKeys();
  try { localStorage.removeItem(LEGACY_AUTHKEY_LS); } catch {}
  await deleteSigningKey().catch(() => {});
};
