// Shell host detection + shared identity contract.
//
// Hey is a capsule that can be hosted by different desktop shells. We
// treat hey-home as the canonical host; stock `home` works but is a
// degraded fallback. This file owns the cross-shell contract:
//
//   .AppData/ElastOS/Identity/profile.json
//     { name, didKey, recoveryKeyHash, passkeys, createdAt, createdBy }
//       ← canonical shared identity. Matches the namespace doc's
//         `.AppData/ElastOS/...` pattern for runtime-owned per-user state
//         (https://github.com/Elacity/elastos-runtime/blob/main/docs/NAMESPACES.md).
//
//   .AppData/Identity/profile.json
//     ← LEGACY location. Older hey-home builds wrote here; upstream may
//       still. We read it as a fallback and keep writing to it during
//       the transition so legacy-aware readers don't lose Hey's writes.
//       Drop the writes once upstream's home capsule has moved.
//
//   .AppData/SystemServices/Shell/active.json
//     ← shell marker. Whichever shell is "active" writes this on boot.
//
// All paths sit OUTSIDE the Hey/ prefix so they land at the per-user
// principal root, where other capsules under the same user can read
// them via /api/apps/<their-name>/storage/.AppData/... (or, for the
// shell, via the privileged /api/localhost/Users/self/* route — both
// hash to the same on-disk location).

import { sharedStorage } from "./runtime";

const SHELL_MARKER_SUFFIX = ".AppData/SystemServices/Shell/active.json";

// Canonical (doc-aligned) and legacy paths for the shared identity.
// Reads prefer canonical; writes hit both during the transition.
const SHARED_IDENTITY_SUFFIX = ".AppData/ElastOS/Identity/profile.json";
const SHARED_IDENTITY_LEGACY_SUFFIX = ".AppData/Identity/profile.json";

const safeRead = async (suffix) => {
  try { return await sharedStorage.readJson(suffix); }
  catch { return null; }
};

const safeWrite = async (suffix, value) => {
  try { await sharedStorage.writeJson(suffix, value); return true; }
  catch { return false; }
};

// ── Shell detection ────────────────────────────────────────────────

let cachedShell = null;

export const detectShell = async () => {
  if (cachedShell) return cachedShell;

  // Primary: marker file written by shells that opt in (hey-home does).
  const marker = await safeRead(SHELL_MARKER_SUFFIX);
  if (marker && marker.name) {
    cachedShell = {
      name: marker.name,
      version: marker.version || null,
      hosted: true,
      source: "marker",
    };
    return cachedShell;
  }

  // Fallback: inspect URL + referrer. Stock home doesn't write a marker
  // but its window path will reveal it.
  const haystack = `${document.referrer || ""} ${window.location.href}`;
  if (/\/apps\/hey-home(\/|$)/.test(haystack)) {
    cachedShell = { name: "hey-home", version: null, hosted: true, source: "url" };
  } else if (/\/apps\/home(\/|$)/.test(haystack)) {
    cachedShell = { name: "home", version: null, hosted: true, source: "url" };
  } else {
    cachedShell = { name: "unknown", version: null, hosted: false, source: "none" };
  }
  return cachedShell;
};

export const isHostedByHeyHome = async () => {
  const s = await detectShell();
  return s.name === "hey-home";
};

export const isHostedByStockHome = async () => {
  const s = await detectShell();
  return s.name === "home";
};

// ── Shared identity ────────────────────────────────────────────────

// Read canonical first; fall back to legacy. Returns null if neither
// path has a profile.
export const readSharedIdentity = async () => {
  const canonical = await safeRead(SHARED_IDENTITY_SUFFIX);
  if (canonical) return canonical;
  return safeRead(SHARED_IDENTITY_LEGACY_SUFFIX);
};

// Write to BOTH paths during the transition. The canonical write is what
// Hey + any future doc-aligned reader uses; the legacy write keeps the
// current upstream home shell seeing Hey's writes.
export const writeSharedIdentity = async (profile) => {
  const a = await safeWrite(SHARED_IDENTITY_SUFFIX, profile);
  const b = await safeWrite(SHARED_IDENTITY_LEGACY_SUFFIX, profile);
  return a || b;
};

// Remove from both paths — leaving stale data anywhere defeats the point.
export const deleteSharedIdentity = async () => {
  await sharedStorage.remove(SHARED_IDENTITY_SUFFIX).catch(() => false);
  await sharedStorage.remove(SHARED_IDENTITY_LEGACY_SUFFIX).catch(() => false);
  return true;
};

// Reset the in-memory cache (used after a "Switch identity" reset).
export const _resetShellCache = () => {
  cachedShell = null;
};
