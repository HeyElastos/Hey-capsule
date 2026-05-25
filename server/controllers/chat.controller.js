// Chat controller — DM-only, did:key-addressed.
//
// Phase 2: works between any two accounts on the same Hey instance.
// Phase 3: identical API; transport layer swaps to Carrier gossip so messages
//          can cross instances. The on-wire shape of each message already
//          matches the SignedEvent envelope, just with signature=null in
//          local mode (server-attested) and signature filled in by the
//          sender for federated mode.
//
// REST surface:
//   GET    /chat/threads                          → list of threads with last message
//   GET    /chat/threads/:peerDid                 → messages for one thread
//   POST   /chat/threads/:peerDid/messages        → send a message
//   POST   /chat/follow                           → add a peer by did:key

const crypto = require("crypto");
const { readDb, writeDb } = require("../utils/db");

const MAX_CONTENT_LEN = 2000;
const PAGE_LIMIT = 100;

// Find the local user record that owns a did:key. Returns undefined for
// did:keys that don't map to any local account (Phase 3 federation will
// expand this to lookup remote peers).
const userByDid = (db, did) => db.users.find((u) => u.didKey === did);

// Make a canonical thread id from a pair of dids. Sorted so the same two
// people produce the same thread regardless of who started it.
const threadIdFor = (didA, didB) => [didA, didB].sort().join("::");

// Public-shaped message for the wire. Mirrors the SignedEvent envelope so
// Phase 3 can move to gossip transport without re-shaping the JSON.
const toPublicMessage = (m) => ({
  id: m.id,
  thread_id: m.threadId,
  sender_did: m.senderDid,
  recipient_did: m.recipientDid,
  content: m.content,
  ts: m.ts,
  signature: m.signature || null,
});

// GET /chat/threads — list every thread the caller participates in,
// newest-message-first, with last-message preview.
const listThreads = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    if (!me) return res.status(404).json({ message: "User not found" });
    if (!me.didKey) {
      return res.status(409).json({
        message: "Your account is missing a federation identity — sign out and back in to backfill it.",
      });
    }

    const myDid = me.didKey;
    const threadMap = new Map();

    for (const m of db.chatMessages) {
      if (m.senderDid !== myDid && m.recipientDid !== myDid) continue;
      const peerDid = m.senderDid === myDid ? m.recipientDid : m.senderDid;
      const existing = threadMap.get(peerDid);
      if (!existing || existing.ts < m.ts) {
        threadMap.set(peerDid, { peerDid, lastMessage: m.content, ts: m.ts });
      }
    }

    // Resolve display info for peer dids that map to local accounts. Unknown
    // dids (federated peers we don't have a local record for) come back with
    // truncated did as the display name.
    const threads = [...threadMap.values()]
      .sort((a, b) => b.ts - a.ts)
      .map(({ peerDid, lastMessage, ts }) => {
        const peer = userByDid(db, peerDid);
        return {
          peer_did: peerDid,
          peer_name: peer?.name || `${peerDid.slice(0, 16)}…`,
          peer_avatar: peer?.avatar || "",
          last_message: lastMessage,
          ts,
        };
      });

    return res.status(200).json({ threads });
  } catch {
    return res.status(500).json({ message: "Failed to load threads" });
  }
};

// GET /chat/threads/:peerDid — paginated message history with one peer.
// ?before=<ts>&limit=<n>
const getThread = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    if (!me?.didKey) return res.status(404).json({ message: "Identity not ready" });

    const peerDid = req.params.peerDid;
    if (!peerDid || !peerDid.startsWith("did:key:z")) {
      return res.status(400).json({ message: "Invalid peer did" });
    }

    const before = req.query.before ? Number(req.query.before) : Infinity;
    const limit = Math.min(Number(req.query.limit) || PAGE_LIMIT, PAGE_LIMIT);

    const tid = threadIdFor(me.didKey, peerDid);
    const messages = db.chatMessages
      .filter((m) => m.threadId === tid && m.ts < before)
      .sort((a, b) => a.ts - b.ts)
      .slice(-limit)
      .map(toPublicMessage);

    const peer = userByDid(db, peerDid);
    return res.status(200).json({
      peer: {
        did: peerDid,
        name: peer?.name || `${peerDid.slice(0, 16)}…`,
        avatar: peer?.avatar || "",
      },
      messages,
    });
  } catch {
    return res.status(500).json({ message: "Failed to load thread" });
  }
};

// POST /chat/threads/:peerDid/messages — send a message. Body: { content }.
// Phase 2 stores server-side with signature=null (local-mode). Phase 3 will
// accept a client-supplied signature and verify before storing.
const sendMessage = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    if (!me?.didKey) {
      return res.status(409).json({ message: "Sign out and back in to enable chat." });
    }

    const peerDid = req.params.peerDid;
    if (!peerDid || !peerDid.startsWith("did:key:z")) {
      return res.status(400).json({ message: "Invalid peer did" });
    }
    if (peerDid === me.didKey) {
      return res.status(400).json({ message: "You can't message yourself" });
    }

    const content = typeof req.body?.content === "string" ? req.body.content.trim() : "";
    if (!content) return res.status(400).json({ message: "Message can't be empty" });
    if (content.length > MAX_CONTENT_LEN) {
      return res.status(413).json({ message: `Message exceeds ${MAX_CONTENT_LEN} chars` });
    }

    const message = {
      id: crypto.randomUUID(),
      threadId: threadIdFor(me.didKey, peerDid),
      senderDid: me.didKey,
      recipientDid: peerDid,
      content,
      ts: Date.now(),
      signature: null, // Phase 3: filled by client-side signer
    };

    db.chatMessages.push(message);
    await writeDb(db);

    return res.status(201).json({ message: toPublicMessage(message) });
  } catch {
    return res.status(500).json({ message: "Failed to send message" });
  }
};

// POST /chat/follow — record a peer did so it shows up as a contact. In
// Phase 2 this is just a sanity-check shortcut to bootstrap a conversation
// before either side has sent a message yet. Phase 3 will tie this to the
// Carrier gossip_join call.
const followPeer = async (req, res) => {
  try {
    const db = await readDb();
    const me = db.users.find((u) => u.id === req.user.id);
    if (!me?.didKey) {
      return res.status(409).json({ message: "Sign out and back in to enable chat." });
    }

    const peerDid = typeof req.body?.did === "string" ? req.body.did.trim() : "";
    if (!peerDid.startsWith("did:key:z")) {
      return res.status(400).json({ message: "Invalid did:key" });
    }
    if (peerDid === me.didKey) {
      return res.status(400).json({ message: "Cannot follow yourself" });
    }

    const peer = userByDid(db, peerDid);
    return res.status(200).json({
      did: peerDid,
      name: peer?.name || `${peerDid.slice(0, 16)}…`,
      avatar: peer?.avatar || "",
      local: Boolean(peer),
    });
  } catch {
    return res.status(500).json({ message: "Failed to follow peer" });
  }
};

module.exports = { listThreads, getThread, sendMessage, followPeer };
