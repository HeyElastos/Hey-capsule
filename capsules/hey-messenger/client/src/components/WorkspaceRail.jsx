import { useStore } from "../state/store.jsx";

export default function WorkspaceRail() {
  const { state, setWorkspace } = useStore();
  return (
    <nav
      className="
        flex flex-col items-center gap-2 py-4 px-2
        w-14 shrink-0
        bg-white/60 dark:bg-zinc-950/60
        backdrop-blur-xl
        border-r border-zinc-200/60 dark:border-zinc-800/60
      "
      aria-label="Workspaces"
    >
      {state.workspaces.map((ws) => {
        const active = ws.id === state.activeWorkspaceId;
        return (
          <button
            key={ws.id}
            onClick={() => setWorkspace(ws.id)}
            title={ws.name}
            className={`
              group relative flex h-10 w-10 items-center justify-center
              rounded-xl text-sm font-semibold text-white
              transition-all
              bg-gradient-to-br ${ws.accent}
              ${active ? "scale-105 ring-2 ring-amber-400/70 ring-offset-2 ring-offset-white dark:ring-offset-zinc-950" : "opacity-80 hover:opacity-100 hover:scale-105"}
            `}
            aria-current={active ? "true" : undefined}
          >
            {ws.initials}
            {active && (
              <span
                aria-hidden
                className="absolute -left-2 top-1/2 -translate-y-1/2 h-6 w-1 rounded-full bg-amber-500"
              />
            )}
          </button>
        );
      })}
      <button
        title="New workspace"
        className="
          mt-1 flex h-10 w-10 items-center justify-center rounded-xl
          border border-dashed border-zinc-300 dark:border-zinc-700
          text-zinc-500 dark:text-zinc-400
          hover:border-amber-400 hover:text-amber-500
          transition-colors
        "
      >
        +
      </button>
    </nav>
  );
}
