import { useEffect, useRef } from "react";
import { useStore } from "../state/store.jsx";
import AttachmentPill from "./AttachmentPill.jsx";
import { Markdown } from "../lib/markdown.js";

const fmtTime = (ts) => {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
};

const avatarColor = (did) => {
  let h = 0;
  for (const c of did) h = (h * 31 + c.charCodeAt(0)) | 0;
  const palette = [
    "from-amber-400 to-orange-500",
    "from-rose-400 to-pink-500",
    "from-emerald-400 to-teal-500",
    "from-sky-400 to-indigo-500",
    "from-violet-400 to-purple-500",
  ];
  return palette[Math.abs(h) % palette.length];
};

const Avatar = ({ name, did }) => (
  <div
    className={`
      h-9 w-9 shrink-0 rounded-full
      bg-gradient-to-br ${avatarColor(did)}
      flex items-center justify-center text-sm font-semibold text-white
      shadow-sm
    `}
  >
    {name?.[0] || "?"}
  </div>
);

// Inline SVG icon for the message hover bar. currentColor inherits
// from the hover bar's text class so dark-mode tinting works.
const CopyIcon = () => (
  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </svg>
);

const HoverAction = ({ title, onClick, children }) => (
  <button
    type="button"
    onClick={onClick}
    title={title}
    aria-label={title}
    className="
      flex h-7 w-7 items-center justify-center rounded-md
      text-zinc-500 dark:text-zinc-400
      hover:bg-amber-500/10 hover:text-amber-600 dark:hover:text-amber-400
      transition-colors
    "
  >
    {children}
  </button>
);

// Floating hover bar — Teams-style row of quick actions. Anchored
// top-right of the message body so it doesn't shift the message's
// layout. `group-hover:opacity-100` reveals on row hover.
//
// Only actions that DO something land here. Reply / React / More
// stay off the bar until they're real features — adding dead buttons
// would lie about capabilities.
const MessageHoverBar = ({ onCopy }) => (
  <div
    className="
      absolute -top-3 right-4
      flex items-center gap-0.5 rounded-lg
      bg-white/95 dark:bg-zinc-800/95
      shadow-lg ring-1 ring-zinc-200/70 dark:ring-zinc-700/70
      backdrop-blur-md
      px-1 py-0.5
      opacity-0 group-hover:opacity-100 focus-within:opacity-100
      transition-opacity duration-150
    "
  >
    <HoverAction title="Copy text" onClick={onCopy}><CopyIcon /></HoverAction>
  </div>
);

const MessageRow = ({ m, isMe, onCopyTicket }) => {
  const copyText = () => {
    if (m.payload?.content && navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(m.payload.content).catch(() => {});
    }
  };
  return (
    <div className="group relative flex items-start gap-3 px-6 py-2 hover:bg-zinc-50/50 dark:hover:bg-zinc-900/30">
      <Avatar name={m.sender_name} did={m.sender_did} />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          <span className={`text-sm font-semibold ${isMe ? "text-amber-700 dark:text-amber-300" : "text-zinc-900 dark:text-zinc-50"}`}>
            {isMe ? "You" : m.sender_name}
          </span>
          <span className="text-[11px] text-zinc-500 dark:text-zinc-400">{fmtTime(m.ts)}</span>
        </div>
        {m.payload?.content && (
          <div className="text-[14px] leading-relaxed text-zinc-800 dark:text-zinc-200 whitespace-pre-wrap break-words">
            <Markdown keyBase={m.id}>{m.payload.content}</Markdown>
          </div>
        )}
        {Array.isArray(m.payload?.attachments) && m.payload.attachments.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-2">
            {m.payload.attachments.map((a, i) => (
              <AttachmentPill
                key={a.cid || a.ticket || i}
                name={a.name}
                size={a.size}
                mime={a.mime}
                status="uploaded"
                ticket={a.ticket}
                onCopy={onCopyTicket}
              />
            ))}
          </div>
        )}
      </div>
      <MessageHoverBar onCopy={copyText} />
    </div>
  );
};

export default function Conversation() {
  const { state } = useStore();
  const allMessages = state.messages[state.activeThreadId] || [];
  const scrollerRef = useRef(null);

  // Apply search filter — case-insensitive substring match against
  // message content. Sender name + attachments also checked so the
  // search feels comprehensive without being clever.
  const q = (state.searchQuery || "").trim().toLowerCase();
  const messages = !q ? allMessages : allMessages.filter((m) => {
    if (m.payload?.content && m.payload.content.toLowerCase().includes(q)) return true;
    if (m.sender_name && m.sender_name.toLowerCase().includes(q)) return true;
    if (Array.isArray(m.payload?.attachments)) {
      for (const a of m.payload.attachments) {
        if (a.name && a.name.toLowerCase().includes(q)) return true;
      }
    }
    return false;
  });

  useEffect(() => {
    const el = scrollerRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messages.length, state.activeThreadId]);

  const onCopyTicket = (ticket) => {
    if (navigator.clipboard?.writeText) navigator.clipboard.writeText(ticket).catch(() => {});
  };

  if (allMessages.length === 0) {
    return (
      <div className="flex flex-1 items-center justify-center text-sm text-zinc-500 dark:text-zinc-400">
        <div className="text-center">
          <div className="text-3xl mb-2">💬</div>
          No messages here yet. Say hi.
        </div>
      </div>
    );
  }

  return (
    <div ref={scrollerRef} className="flex-1 overflow-y-auto py-3">
      {q && (
        <div className="px-6 pb-2 text-[11px] text-zinc-500 dark:text-zinc-400">
          {messages.length} of {allMessages.length} messages match
          <span className="ml-1 text-amber-700 dark:text-amber-400 font-medium">“{q}”</span>
        </div>
      )}
      {messages.length === 0 ? (
        <div className="px-6 py-12 text-center text-sm text-zinc-500 dark:text-zinc-400">
          No messages match this search.
        </div>
      ) : (
        messages.map((m) => (
          <MessageRow
            key={m.id}
            m={m}
            isMe={m.sender_did === state.currentUser.did}
            onCopyTicket={onCopyTicket}
          />
        ))
      )}
    </div>
  );
}
