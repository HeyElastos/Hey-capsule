import { useState } from "react";
import { useStore } from "../state/store.jsx";

const PresenceDot = ({ presence }) => {
  const color =
    presence === "online" ? "bg-emerald-500"
    : presence === "idle" ? "bg-amber-400"
    : "bg-zinc-400 dark:bg-zinc-600";
  return (
    <span
      className={`inline-block h-2 w-2 rounded-full ring-2 ring-white dark:ring-zinc-900 ${color}`}
    />
  );
};

const Chevron = ({ open }) => (
  <svg
    width="10"
    height="10"
    viewBox="0 0 16 16"
    className={`transition-transform duration-150 ${open ? "rotate-90" : ""}`}
    fill="currentColor"
    aria-hidden="true"
  >
    <path d="M5 3l6 5-6 5z" />
  </svg>
);

const SectionHeader = ({ open, onToggle, count, children }) => (
  <button
    onClick={onToggle}
    className="
      group flex w-full items-center gap-1.5
      px-2.5 pt-3 pb-1.5
      text-[10px] font-semibold uppercase tracking-wider
      text-zinc-500 dark:text-zinc-400
      hover:text-zinc-700 dark:hover:text-zinc-200
      transition-colors
    "
  >
    <Chevron open={open} />
    <span className="flex-1 truncate text-left">{children}</span>
    {typeof count === "number" && (
      <span className="text-zinc-400 dark:text-zinc-500 font-medium">{count}</span>
    )}
  </button>
);

const Row = ({ active, onClick, children, badge }) => (
  <button
    onClick={onClick}
    className={`
      group flex w-full items-center gap-2 rounded-md
      px-2.5 py-1 text-[13px]
      transition-colors
      ${active
        ? "bg-amber-500/15 text-zinc-900 dark:text-zinc-50 font-medium"
        : "text-zinc-700 dark:text-zinc-300 hover:bg-zinc-200/50 dark:hover:bg-zinc-800/50"}
    `}
  >
    <span className="flex-1 truncate text-left">{children}</span>
    {badge ? (
      <span className="inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-amber-500 px-1.5 text-[10px] font-semibold text-white">
        {badge}
      </span>
    ) : null}
  </button>
);

// Light divider between collapsible groups.
const Divider = () => (
  <div className="mx-2.5 my-1 h-px bg-zinc-200/70 dark:bg-zinc-800/70" />
);

export default function ChannelList() {
  const { state, setThread } = useStore();
  const ws = state.workspaces.find((w) => w.id === state.activeWorkspaceId);
  const channels = state.channelsByWorkspace[state.activeWorkspaceId] || [];
  const dms = state.dmsByWorkspace[state.activeWorkspaceId] || [];

  // Per-section open state. Default both open. Persists within the
  // session — moving between workspaces preserves the user's choice.
  // Not in the store because it's purely view-local; sync to upstream
  // when we have a settings sink.
  const [channelsOpen, setChannelsOpen] = useState(true);
  const [dmsOpen, setDmsOpen] = useState(true);

  return (
    <aside
      className="
        w-64 shrink-0 flex flex-col
        bg-white/50 dark:bg-zinc-900/40
        backdrop-blur-xl
        border-r border-zinc-200/60 dark:border-zinc-800/60
      "
    >
      {/* Workspace title — the "team" header in Teams' hierarchy. */}
      <div className="px-3 py-3 border-b border-zinc-200/60 dark:border-zinc-800/60">
        <div className="flex items-center gap-2">
          <div className="text-sm font-semibold tracking-tight truncate flex-1">
            {ws?.name}
          </div>
        </div>
        <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
          {channels.length} channels · {dms.length} DMs
        </div>
      </div>

      {/* Scroll body: expandable Channels + DM groups. Tighter row
          spacing than before — Teams reads at info-density, not
          comfort-density. */}
      <div className="flex-1 overflow-y-auto px-1.5 pb-3">
        <SectionHeader
          open={channelsOpen}
          onToggle={() => setChannelsOpen((v) => !v)}
          count={channels.length}
        >
          Channels
        </SectionHeader>
        {channelsOpen && channels.map((c) => (
          <Row
            key={c.id}
            active={c.id === state.activeThreadId}
            onClick={() => setThread(c.id)}
            badge={c.unread || undefined}
          >
            <span className="text-zinc-500 dark:text-zinc-500">#</span> {c.name}
          </Row>
        ))}

        {dms.length > 0 && (
          <>
            <Divider />
            <SectionHeader
              open={dmsOpen}
              onToggle={() => setDmsOpen((v) => !v)}
              count={dms.length}
            >
              Direct messages
            </SectionHeader>
            {dmsOpen && dms.map((d) => (
              <Row
                key={d.id}
                active={d.id === state.activeThreadId}
                onClick={() => setThread(d.id)}
              >
                <span className="inline-flex items-center gap-2">
                  <PresenceDot presence={d.presence} />
                  {d.name}
                </span>
              </Row>
            ))}
          </>
        )}
      </div>
    </aside>
  );
}
