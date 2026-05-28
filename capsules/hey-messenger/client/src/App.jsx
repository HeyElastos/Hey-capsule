import { useEffect } from "react";
import { StoreProvider, useStore } from "./state/store.jsx";
import WorkspaceRail from "./components/WorkspaceRail.jsx";
import ChannelList from "./components/ChannelList.jsx";
import Conversation from "./components/Conversation.jsx";
import Composer from "./components/Composer.jsx";
import Inspector from "./components/Inspector.jsx";
import EncryptionBadge from "./components/EncryptionBadge.jsx";
import { startInbox, dmTopic, channelTopic } from "./lib/inbox.js";
import { publishOwnBundle } from "./lib/profile.js";
import { getDidKey } from "./lib/session.js";

const ChannelHeader = () => {
  const { state, toggleInspector, setSearch } = useStore();
  const chans = state.channelsByWorkspace[state.activeWorkspaceId] || [];
  const dms   = state.dmsByWorkspace[state.activeWorkspaceId]   || [];
  const c = chans.find((x) => x.id === state.activeThreadId);
  const d = dms.find((x) => x.id === state.activeThreadId);
  const name = c ? `# ${c.name}` : d ? d.name : "—";
  const subtitle = c ? "channel" : d ? "direct message" : "";
  // E2E is only honest when (a) this is a DM and (b) we have the peer's
  // resolved pubkey bundle. Mock DMs carry no DID so they stay transit-only.
  const encKind = d && d.did ? "e2e" : "transit";
  return (
    <header
      className="
        flex items-center gap-3
        px-5 py-3
        bg-white/40 dark:bg-zinc-900/30
        backdrop-blur-xl
        border-b border-zinc-200/60 dark:border-zinc-800/60
      "
    >
      <div className="flex items-center gap-3 min-w-0">
        <div className="min-w-0">
          <div className="text-base font-semibold tracking-tight truncate">{name}</div>
          <div className="text-[11px] text-zinc-500 dark:text-zinc-400">{subtitle}</div>
        </div>
        <EncryptionBadge kind={encKind} />
      </div>

      {/* Search — filters the current channel's messages on the
          client. Real filter, real value cleared on thread switch. */}
      <div className="flex-1 max-w-sm ml-auto">
        <div className="relative">
          <span className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-400 dark:text-zinc-500 pointer-events-none">
            <SearchIcon />
          </span>
          <input
            type="search"
            value={state.searchQuery}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={`Search in ${c ? `#${c.name}` : d ? d.name : "thread"}`}
            className="
              w-full rounded-lg pl-9 pr-3 py-1.5 text-[13px]
              bg-zinc-100/70 dark:bg-zinc-800/60
              border border-zinc-200/60 dark:border-zinc-700/60
              text-zinc-900 dark:text-zinc-100
              placeholder:text-zinc-400 dark:placeholder:text-zinc-500
              outline-none focus:border-amber-400 focus:ring-2 focus:ring-amber-400/30
              transition
            "
          />
        </div>
      </div>

      <div className="flex items-center gap-1">
        {/* Video call — coming in Phase 2. Disabled + tooltip is the
            honest signal; not a fake button that swallows clicks. */}
        <button
          disabled
          title="Video calls — coming soon (P2P over Carrier-signaled WebRTC)"
          aria-label="Video calls — coming soon"
          className="
            relative rounded-lg p-1.5 text-zinc-400 dark:text-zinc-500
            cursor-not-allowed opacity-60
          "
        >
          <VideoIcon />
          <span className="absolute -top-1 -right-1 rounded-full bg-amber-500/90 text-[8px] font-bold uppercase tracking-wider text-white px-1 py-[1px] leading-none">
            soon
          </span>
        </button>
        <IconBtn title="Toggle inspector" onClick={toggleInspector}>
          <PanelIcon />
        </IconBtn>
      </div>
    </header>
  );
};

const IconBtn = ({ children, title, onClick }) => (
  <button
    title={title}
    onClick={onClick}
    className="rounded-lg p-1.5 text-zinc-500 hover:bg-amber-500/10 hover:text-amber-600 dark:text-zinc-400 dark:hover:text-amber-400 transition-colors"
  >
    {children}
  </button>
);

const Backdrop = ({ children }) => (
  <div
    className="
      relative h-full w-full overflow-hidden
      bg-gradient-to-br
      from-amber-50 via-rose-50 to-zinc-100
      dark:from-zinc-950 dark:via-zinc-950 dark:to-zinc-900
    "
  >
    <div aria-hidden className="pointer-events-none absolute -top-32 -left-32 h-96 w-96 rounded-full bg-amber-400/20 blur-3xl dark:bg-amber-500/10" />
    <div aria-hidden className="pointer-events-none absolute -bottom-32 -right-32 h-96 w-96 rounded-full bg-rose-400/20 blur-3xl dark:bg-rose-500/10" />
    {children}
  </div>
);

// Compute the live list of Carrier topics for the active workspace.
// The inbox poller calls this each tick so adding/removing channels
// or DMs takes effect without re-mounting.
const buildTopicList = (state) => {
  const myDid = state.currentUser.did;
  const channels = state.channelsByWorkspace[state.activeWorkspaceId] || [];
  const dms = state.dmsByWorkspace[state.activeWorkspaceId] || [];
  const out = [];
  for (const c of channels) {
    out.push({
      id: c.id,
      topic: channelTopic(state.activeWorkspaceId, c.id),
      kind: "channel",
    });
  }
  for (const d of dms) {
    out.push({
      id: d.id,
      topic: d.did ? dmTopic(myDid, d.did) : `hey-msg/v0/dm-mock/${d.id}`,
      kind: "dm",
    });
  }
  return out;
};

const Shell = () => {
  const { state, appendMessage } = useStore();

  // Boot the inbox poller + publish our profile bundle once.
  useEffect(() => {
    if (!getDidKey()) {
      // Not signed in — skip wiring; user can still browse the UI shell.
      return;
    }
    publishOwnBundle().catch((err) => {
      console.warn("[hey-messenger] profile bundle publish failed", err);
    });
    const stop = startInbox({
      topics: () => buildTopicList(state),
      onMessage: ({ threadId, message }) => {
        // Pull plaintext out — decrypted-payload first, encrypted-but-unreadable
        // gets a placeholder. Skip our own echoes (inbox already filters by
        // skip_sender_id but defense in depth).
        if (message.sender_did === state.currentUser.did) return;
        const payload = message.payload_decrypted ?? message.payload ?? {};
        appendMessage(threadId, {
          id: message.id || `remote-${message.ts}-${message.sender_did?.slice(-6)}`,
          sender_did: message.sender_did,
          sender_name: message.sender_name || message.sender_did?.slice(0, 14) + "…",
          ts: message.ts,
          payload: message.encryptedButUnreadable
            ? { content: "🔒 encrypted message — no key", _unreadable: true }
            : payload,
        });
      },
    });
    return () => stop();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.activeWorkspaceId, state.currentUser.did]);

  return (
    <Backdrop>
      <div className="relative z-10 flex h-full">
        <WorkspaceRail />
        <ChannelList />
        <main className="flex flex-1 flex-col min-w-0">
          <ChannelHeader />
          <Conversation />
          <Composer />
        </main>
        {state.inspectorOpen && <Inspector />}
      </div>
    </Backdrop>
  );
};

export default function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}

// — icons —
const VideoIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <polygon points="23 7 16 12 23 17 23 7" />
    <rect x="1" y="5" width="15" height="14" rx="2" ry="2" />
  </svg>
);
const SearchIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <circle cx="11" cy="11" r="7" />
    <line x1="21" y1="21" x2="16.65" y2="16.65" />
  </svg>
);
const PanelIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
    <rect x="3" y="3" width="18" height="18" rx="2" />
    <line x1="15" y1="3" x2="15" y2="21" />
  </svg>
);
