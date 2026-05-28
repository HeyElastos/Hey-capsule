import { useState } from "react";
import { useStore } from "../state/store.jsx";
import {
  signInViaRuntime,
  signInWithGeneratedKey,
  passkeySupported,
} from "../api/passkey.js";

// Gate component: blocks rendering of the messenger Shell until the
// user has signed in (getDidKey() returns a real DID via the store).
// Primary path is "Sign in with passkey" — calls upstream's
// /api/auth/passkey/authenticate/{begin,complete} via signInViaRuntime.
// Secondary path is a recovery-key generator for users without a
// passkey-capable authenticator.
//
// Same UX as Hey Social's Landing, simpler (no FloatingScene SVG, no
// HeyMark wordmark — messenger doesn't have those assets).

export default function SignInGate({ children }) {
  const { state, ready, refreshCurrentUser } = useStore();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(null);

  // Recovery-key fallback state.
  const [showRecovery, setShowRecovery] = useState(false);
  const [recoveryName, setRecoveryName] = useState("");
  const [generatedKey, setGeneratedKey] = useState(null);
  const [keyCopied, setKeyCopied] = useState(false);

  // While store loads, show a splash so Shell doesn't render briefly.
  if (!ready) {
    return (
      <div className="flex h-full w-full items-center justify-center bg-zinc-100 dark:bg-zinc-950 text-sm text-zinc-500 dark:text-zinc-400">
        Loading…
      </div>
    );
  }

  // Already signed in → render the actual messenger.
  if (state.currentUser?.did && state.currentUser.canSign) {
    return children;
  }

  const handlePasskey = async () => {
    setError(null);
    setBusy(true);
    try {
      await signInViaRuntime();
      refreshCurrentUser(); // re-read session.getDidKey()
    } catch (err) {
      const msg = err?.message || "Passkey sign-in failed.";
      if (/NotAllowedError|AbortError|cancelled|canceled/i.test(msg)) {
        setError("Passkey prompt closed. Tap to try again.");
      } else {
        setError(msg);
      }
    } finally {
      setBusy(false);
    }
  };

  const handleGenerateKey = async () => {
    setError(null);
    if (!recoveryName.trim()) {
      setError("Pick a nickname.");
      return;
    }
    setBusy(true);
    try {
      const r = await signInWithGeneratedKey({ name: recoveryName.trim() });
      setGeneratedKey(r.authKey);
    } catch (err) {
      setError(err?.message || "Could not generate key.");
    } finally {
      setBusy(false);
    }
  };

  const handleCopyKey = async () => {
    if (!generatedKey || !navigator.clipboard?.writeText) return;
    try {
      await navigator.clipboard.writeText(generatedKey);
      setKeyCopied(true);
      setTimeout(() => setKeyCopied(false), 1500);
    } catch (_) {}
  };

  const handleContinue = () => {
    refreshCurrentUser();
  };

  const canUsePasskey = passkeySupported();

  return (
    <div className="relative flex h-full w-full items-center justify-center px-4 py-8 bg-gradient-to-br from-amber-50 via-rose-50 to-zinc-100 dark:from-zinc-950 dark:via-zinc-950 dark:to-zinc-900">
      <div aria-hidden className="pointer-events-none absolute -top-32 -left-32 h-96 w-96 rounded-full bg-amber-400/20 blur-3xl dark:bg-amber-500/10" />
      <div aria-hidden className="pointer-events-none absolute -bottom-32 -right-32 h-96 w-96 rounded-full bg-rose-400/20 blur-3xl dark:bg-rose-500/10" />

      <div className="relative z-10 w-full max-w-md">
        <div className="text-center mb-8">
          <h1 className="text-5xl font-bold tracking-tight text-zinc-900 dark:text-zinc-50">
            Hey <span className="text-amber-500">Chat</span>
          </h1>
          <p className="mt-3 text-sm text-zinc-600 dark:text-zinc-400">
            P2P messenger over Elastos. End-to-end encrypted with hybrid
            post-quantum crypto. No server in the middle.
          </p>
        </div>

        <div className="rounded-2xl bg-white/80 dark:bg-zinc-900/70 backdrop-blur-xl border border-zinc-200/70 dark:border-zinc-800/70 p-6 shadow-xl">
          {!generatedKey && canUsePasskey && (
            <>
              <button
                type="button"
                onClick={handlePasskey}
                disabled={busy}
                className="w-full inline-flex items-center justify-center gap-2 rounded-full bg-amber-500 hover:bg-amber-600 disabled:bg-zinc-300 dark:disabled:bg-zinc-700 disabled:cursor-not-allowed text-white font-semibold px-6 py-3.5 text-base shadow-lg transition-colors"
              >
                <svg viewBox="0 0 24 24" className="h-5 w-5 fill-current">
                  <path d="M12 2a5 5 0 0 0-5 5v3H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-8a2 2 0 0 0-2-2h-1V7a5 5 0 0 0-5-5Zm-3 8V7a3 3 0 0 1 6 0v3H9Z" />
                </svg>
                {busy ? "Waiting for passkey…" : "Sign in with passkey"}
              </button>
              <p className="mt-3 text-center text-[12px] text-zinc-500 dark:text-zinc-400">
                Same passkey as System. One tap, nothing to remember.
              </p>
            </>
          )}

          {!generatedKey && !canUsePasskey && (
            <div className="text-sm text-zinc-600 dark:text-zinc-400 text-center">
              Your browser doesn't support passkeys. Use a modern Chrome /
              Edge / Safari / Firefox to sign in.
            </div>
          )}

          {error && (
            <p className="mt-4 text-sm text-red-500 dark:text-red-400 text-center">
              {error}
            </p>
          )}

          {!generatedKey && (
            <div className="mt-6 pt-6 border-t border-zinc-200/70 dark:border-zinc-800/70">
              {!showRecovery ? (
                <button
                  type="button"
                  onClick={() => setShowRecovery(true)}
                  className="block mx-auto text-[12px] text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200 underline-offset-4 hover:underline transition-colors"
                >
                  No passkey? Use a recovery key instead
                </button>
              ) : (
                <div className="space-y-3">
                  <div className="flex items-baseline justify-between">
                    <p className="text-[11px] uppercase tracking-wider text-zinc-500 dark:text-zinc-400">
                      Recovery key
                    </p>
                    <button
                      type="button"
                      onClick={() => { setShowRecovery(false); setRecoveryName(""); setError(null); }}
                      className="text-[11px] text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200"
                    >
                      Hide
                    </button>
                  </div>
                  <p className="text-[12px] leading-relaxed text-zinc-500 dark:text-zinc-400">
                    We'll generate a 32-byte secret you must save. Lose it,
                    lose your account.
                  </p>
                  <input
                    type="text"
                    value={recoveryName}
                    onChange={(e) => setRecoveryName(e.target.value)}
                    disabled={busy}
                    maxLength={30}
                    placeholder="Pick a nickname"
                    className="w-full rounded-lg bg-zinc-100 dark:bg-zinc-800 border border-zinc-200 dark:border-zinc-700 px-3 py-2 text-sm text-zinc-900 dark:text-zinc-100 placeholder:text-zinc-400 dark:placeholder:text-zinc-500 outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-400/30"
                  />
                  <button
                    type="button"
                    onClick={handleGenerateKey}
                    disabled={busy || !recoveryName.trim()}
                    className="w-full rounded-full border border-zinc-200 dark:border-zinc-700 bg-white dark:bg-zinc-800 px-4 py-2 text-sm font-semibold text-zinc-700 dark:text-zinc-300 hover:bg-zinc-50 dark:hover:bg-zinc-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                  >
                    {busy ? "Generating…" : "Generate a recovery key"}
                  </button>
                </div>
              )}
            </div>
          )}

          {generatedKey && (
            <div className="space-y-3">
              <div className="flex items-center gap-2">
                <span className="inline-flex h-2 w-2 animate-pulse rounded-full bg-emerald-500" />
                <p className="text-[11px] uppercase tracking-wider text-emerald-600 dark:text-emerald-300">
                  Welcome, {recoveryName.trim() || "You"}
                </p>
              </div>
              <p className="text-sm text-zinc-600 dark:text-zinc-400">
                This is your recovery key.{" "}
                <strong className="text-zinc-900 dark:text-zinc-50">Save it now</strong>{" "}
                — it's the only way to sign back in on another device.
              </p>
              <p className="select-all break-all rounded-lg bg-zinc-100 dark:bg-zinc-800 px-3 py-2 text-center font-mono text-[12px] text-zinc-700 dark:text-zinc-300">
                {generatedKey}
              </p>
              <button
                type="button"
                onClick={handleCopyKey}
                className="w-full rounded-full bg-amber-500 hover:bg-amber-600 text-white font-semibold px-4 py-2.5 text-sm transition-colors"
              >
                {keyCopied ? "Copied ✓" : "Copy key"}
              </button>
              <button
                type="button"
                onClick={handleContinue}
                className="w-full rounded-full bg-emerald-500 hover:bg-emerald-600 text-white font-semibold px-4 py-2.5 text-sm transition-colors"
              >
                I saved it — continue
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
