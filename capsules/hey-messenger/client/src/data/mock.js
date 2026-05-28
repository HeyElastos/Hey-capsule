// Mock data for the Phase 3 UI. Replace with iroh-docs queries once the
// docs-provider is in place. Shape mirrors what real signed events will
// look like coming off Carrier, so swap-out is mechanical.

export const workspaces = [
  { id: "ws-acme",   name: "Acme",      initials: "A", accent: "from-amber-500 to-orange-600" },
  { id: "ws-design", name: "Design Co", initials: "D", accent: "from-rose-500 to-pink-600" },
  { id: "ws-self",   name: "Personal",  initials: "P", accent: "from-emerald-500 to-teal-600" },
];

export const channelsByWorkspace = {
  "ws-acme": [
    { id: "c-general",  name: "general",     unread: 0 },
    { id: "c-eng",      name: "engineering", unread: 3 },
    { id: "c-design",   name: "design",      unread: 0 },
    { id: "c-random",   name: "random",      unread: 12 },
    { id: "c-launch",   name: "launch-2026", unread: 0 },
  ],
  "ws-design": [
    { id: "c-design-general", name: "general",  unread: 0 },
    { id: "c-design-crit",    name: "critique", unread: 1 },
  ],
  "ws-self": [
    { id: "c-self-notes", name: "notes-to-self", unread: 0 },
  ],
};

export const dmsByWorkspace = {
  "ws-acme": [
    { id: "dm-alice", name: "Alice Chen",   presence: "online" },
    { id: "dm-bob",   name: "Bob Tanaka",   presence: "idle" },
    { id: "dm-carol", name: "Carol Riveiro", presence: "offline" },
  ],
  "ws-design": [
    { id: "dm-pat", name: "Pat Linden", presence: "online" },
  ],
  "ws-self": [],
};

// Threads keyed by channel/DM id. Each message has the on-wire shape we
// expect from signed Carrier events — sender_did + ts + payload — minus
// the actual signature (mock data isn't signed).
export const messagesByThread = {
  "c-general": [
    {
      id: "m1",
      sender_did: "did:key:z6MkAlice...",
      sender_name: "Alice Chen",
      ts: Date.now() - 1000 * 60 * 38,
      payload: { content: "morning team — pushed the v3 spec, take a look when you have a sec" },
    },
    {
      id: "m2",
      sender_did: "did:key:z6MkAlice...",
      sender_name: "Alice Chen",
      ts: Date.now() - 1000 * 60 * 37,
      payload: {
        content: "tldr: we're going with iroh-blobs for file share, ditches the 100MB nginx ceiling",
        attachments: [
          {
            cid: "demo-spec-pdf",
            ticket: "blobacyjnebt6yp2fa242u4pyxd6bomovg74…",
            name: "acme-v3-spec.pdf",
            size: 240_400_000,
            mime: "application/pdf",
          },
        ],
      },
    },
    {
      id: "m3",
      sender_did: "did:key:z6MkBob...",
      sender_name: "Bob Tanaka",
      ts: Date.now() - 1000 * 60 * 12,
      payload: { content: "reading now 👀" },
    },
    {
      id: "m4",
      sender_did: "did:key:z6MkBob...",
      sender_name: "Bob Tanaka",
      ts: Date.now() - 1000 * 60 * 4,
      payload: { content: "yeah this is the right call. the base64 staging was killing us on the 80MB video reviews" },
    },
  ],
  "c-eng": [
    {
      id: "m1",
      sender_did: "did:key:z6MkBob...",
      sender_name: "Bob Tanaka",
      ts: Date.now() - 1000 * 60 * 60 * 2,
      payload: { content: "rolling rustc 1.91 across the providers — heads up if you `cargo build` locally" },
    },
  ],
  "c-design": [],
  "c-random": [
    {
      id: "m1",
      sender_did: "did:key:z6MkCarol...",
      sender_name: "Carol Riveiro",
      ts: Date.now() - 1000 * 60 * 60 * 5,
      payload: { content: "anyone else having the Tuesday energy on a Wednesday" },
    },
  ],
  "c-launch": [],
  "dm-alice": [
    {
      id: "m1",
      sender_did: "did:key:z6MkAlice...",
      sender_name: "Alice Chen",
      ts: Date.now() - 1000 * 60 * 90,
      payload: { content: "can you review the call architecture before the standup?" },
    },
  ],
  "dm-bob":   [],
  "dm-carol": [],
};

export const currentUser = {
  did: "did:key:z6MkMe...",
  name: "You",
};

export const allThreadsList = (wsId) => [
  ...(channelsByWorkspace[wsId] || []).map((c) => ({ ...c, kind: "channel" })),
  ...(dmsByWorkspace[wsId]   || []).map((d) => ({ ...d, kind: "dm" })),
];
