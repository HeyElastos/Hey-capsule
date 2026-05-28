const formatBytes = (n) => {
  if (!n && n !== 0) return "";
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
};

const iconFor = (mime = "") => {
  if (mime.startsWith("image/")) return "🖼";
  if (mime.startsWith("video/")) return "🎬";
  if (mime.startsWith("audio/")) return "🎵";
  if (mime === "application/pdf") return "📄";
  if (mime.startsWith("application/zip") || mime.includes("compressed")) return "🗜";
  return "📎";
};

export default function AttachmentPill({ name, size, mime, status = "ready", progress, ticket, onCopy }) {
  return (
    <div
      className="
        inline-flex max-w-md items-center gap-3 rounded-xl
        bg-white/70 dark:bg-zinc-900/60
        backdrop-blur-md
        border border-zinc-200/70 dark:border-zinc-800/70
        px-3 py-2 text-sm
        shadow-sm
      "
    >
      <div className="text-xl leading-none">{iconFor(mime)}</div>
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium text-zinc-900 dark:text-zinc-50">{name}</div>
        <div className="text-[11px] text-zinc-500 dark:text-zinc-400">
          {formatBytes(size)}
          {status === "uploading" && progress != null
            ? ` · uploading ${Math.round(progress * 100)}%`
            : status === "uploaded"
            ? " · sent via iroh-blobs"
            : status === "error"
            ? " · upload failed"
            : ""}
        </div>
      </div>
      {status === "uploaded" && ticket ? (
        <button
          onClick={() => onCopy?.(ticket)}
          title="Copy ticket"
          className="
            rounded-md border border-zinc-200/70 dark:border-zinc-700/70
            px-2 py-1 text-[11px] font-medium text-zinc-700 dark:text-zinc-200
            hover:bg-amber-500/10 hover:border-amber-400/50 hover:text-amber-700 dark:hover:text-amber-300
            transition-colors
          "
        >
          copy ticket
        </button>
      ) : status === "uploading" ? (
        <div className="h-4 w-4 animate-spin rounded-full border-2 border-amber-500 border-t-transparent" />
      ) : null}
    </div>
  );
}
