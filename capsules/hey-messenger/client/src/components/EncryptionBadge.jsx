// Honest encryption posture indicator.
//
//   🔒 E2E       — Hybrid X25519 + ML-KEM-768 + ChaCha20-Poly1305.
//                  Only the recipient can read; passive observers see
//                  only ciphertext. Used for DMs once peer profile
//                  is resolvable.
//   🔓 Transit   — Carrier QUIC (TLS 1.3) hop-to-hop. Bytes are
//                  encrypted in flight but anyone who joins the
//                  topic reads the plaintext. Used for groups (MLS
//                  is Phase 6) and DMs where we don't yet have the
//                  peer's pubkey bundle.

export default function EncryptionBadge({ kind, tooltip }) {
  const styles =
    kind === "e2e"
      ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300 border-emerald-500/30"
      : "bg-amber-500/10 text-amber-700 dark:text-amber-400 border-amber-500/25";
  const label =
    kind === "e2e" ? "E2E · hybrid PQ" : "transit-only";
  const icon = kind === "e2e" ? "🔒" : "🔓";
  return (
    <span
      title={tooltip || (kind === "e2e"
        ? "Hybrid post-quantum end-to-end: X25519 + ML-KEM-768 + ChaCha20-Poly1305. Only the recipient can read."
        : "Encrypted hop-to-hop in transit (QUIC + TLS 1.3) but anyone in the topic reads the plaintext. Group E2E (MLS) is a tracked follow-up.")}
      className={`
        inline-flex items-center gap-1 rounded-md border
        px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide
        ${styles}
      `}
    >
      <span aria-hidden>{icon}</span>
      {label}
    </span>
  );
}
