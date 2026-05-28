// Inbox poller — pulls signed events off Carrier topics, verifies them,
// decrypts hybrid-PQ envelopes when our keys match, and hands each result
// to the caller's onMessage callback.
//
// This module is React-agnostic. The UI wires it up by passing an
// onMessage function that dispatches into the React store.
//
// Polling cadence: 1000 ms is fine for a chat UI. Real-time updates
// would need streaming (SSE / WS) from the peer provider; that's
// tracked as a follow-up.

import { peer } from "./runtime.js";
import { verifySignedEvent } from "./events.js";
import { decryptHybrid } from "./pqcrypto.js";
import { getKeypair, getDidKey } from "./session.js";

const DEFAULT_INTERVAL_MS = 1000;
const RECV_LIMIT = 50;

// Topic-name helpers ─────────────────────────────────────────────────

// DM topic is canonical: both peers publish/listen on the same string.
// Sort the two DIDs so order doesn't matter.
export const dmTopic = (didA, didB) => {
  const [a, b] = [didA, didB].sort();
  return `hey-msg/v0/dm/${a}::${b}`;
};

export const channelTopic = (workspaceId, channelId) =>
  `hey-msg/v0/ws/${workspaceId}/ch/${channelId}/msg`;

// Per-thread state — last seen ts and last error.
const threadState = new Map();

// Start a polling loop. Returns a stop() function.
//
//   topics:    () => Array<{ id, topic, kind }>     ; recomputed each tick
//                                                     so the caller can add /
//                                                     remove topics live.
//   onMessage: ({ threadId, message, encrypted }) => void
//
// `message` is the on-wire shape with `sender_did`, `ts`, and either
// `payload` (plaintext) or `payload_decrypted` (after decrypt). For
// encrypted events we couldn't decrypt, `payload_decrypted` is omitted
// and `encryptedButUnreadable` is true so the UI can badge it.
export const startInbox = ({ topics, onMessage, intervalMs = DEFAULT_INTERVAL_MS }) => {
  let stopped = false;
  const consumerId = `hey-messenger:inbox:${getDidKey() || "anon"}`;

  const tick = async () => {
    if (stopped) return;
    const list = (() => { try { return topics() || []; } catch { return []; } })();
    for (const t of list) {
      if (stopped) return;
      await pollOne(t, consumerId, onMessage);
    }
  };

  const interval = setInterval(tick, intervalMs);
  // Kick the first tick immediately so the UI populates without waiting.
  tick();

  return () => {
    stopped = true;
    clearInterval(interval);
  };
};

const pollOne = async (t, consumerId, onMessage) => {
  const selfDid = getDidKey();
  let resp;
  try {
    resp = await peer.recv({
      topic: t.topic,
      limit: RECV_LIMIT,
      consumer_id: consumerId,
      skip_sender_id: selfDid || undefined,
    });
  } catch (err) {
    // Provider not available yet — that's fine, try again on next tick.
    threadState.set(t.id, { err: String(err.message || err), at: Date.now() });
    return;
  }
  const items = resp?.data?.messages || resp?.messages || [];
  if (!items.length) return;

  const kp = getKeypair();
  for (const item of items) {
    let event;
    try { event = JSON.parse(item.message ?? item); } catch { continue; }
    if (!event || typeof event !== "object") continue;
    if (event.type === "profile.bundle") continue; // not chat content
    const v = verifySignedEvent(event);
    if (!v.valid) continue;

    // Decrypt if the payload looks like a hybrid envelope.
    const env = event.payload?.enc;
    if (env && env.v === "hpq-1") {
      if (!kp?.x25519?.privateKey || !kp?.kem?.secretKey) {
        onMessage({
          threadId: t.id,
          message: { ...event, encryptedButUnreadable: true, reason: "no-key" },
        });
        continue;
      }
      try {
        const plaintext = decryptHybrid(env, kp.x25519.privateKey, kp.kem.secretKey);
        let parsed = plaintext;
        try { parsed = JSON.parse(plaintext); } catch {}
        onMessage({
          threadId: t.id,
          encrypted: true,
          message: { ...event, payload_decrypted: parsed },
        });
      } catch (err) {
        onMessage({
          threadId: t.id,
          message: {
            ...event,
            encryptedButUnreadable: true,
            reason: String(err.message || err),
          },
        });
      }
    } else {
      onMessage({ threadId: t.id, message: event });
    }
  }
};
