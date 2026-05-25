// Session — persists the user's authKey across page reloads in capsule mode.
//
// Signing federated events (chat messages, post events, profile updates)
// requires the Ed25519 seed. Server mode never sees the seed client-side
// because the server signs on behalf of the user. Capsule mode has no
// server — we have to keep the seed reachable from the browser.
//
// Tradeoff: localStorage is XSS-reachable. Hey's threat model accepts this
// for now since the entire app context already has access to whatever it
// needs to impersonate the user via API calls. A future hardening pass
// can move to IndexedDB with a non-extractable WebAuthn-wrapped key.

import { expandKeypair } from "./identity";

const SESSION_KEY = "hey-capsule-session";
let cached = null;

export const setSession = (authKey) => {
  if (!authKey) return clearSession();
  cached = { authKey, ...expandKeypair(authKey) };
  try {
    localStorage.setItem(SESSION_KEY, JSON.stringify({ authKey }));
  } catch { /* private-mode storage refusal — fall back to in-memory only */ }
};

export const getKeypair = () => {
  if (cached) return cached;
  let stored;
  try {
    stored = JSON.parse(localStorage.getItem(SESSION_KEY) || "null");
  } catch { stored = null; }
  if (!stored?.authKey) return null;
  cached = { authKey: stored.authKey, ...expandKeypair(stored.authKey) };
  return cached;
};

export const getDidKey = () => getKeypair()?.didKey || null;

export const clearSession = () => {
  cached = null;
  try { localStorage.removeItem(SESSION_KEY); } catch { /* ignore */ }
};
