import { Link } from "react-router-dom";

const mockedVideos = [
  {
    id: "r1",
    title: "Sunset city drive",
    creator: "Ava",
    description: "A quick visual story from the evening commute.",
    gradient: "linear-gradient(135deg, #f59e0b 0%, #ef4444 60%, #4c1d95 100%)",
  },
  {
    id: "r2",
    title: "Studio details",
    creator: "Milo",
    description: "A short behind-the-scenes creative snapshot.",
    gradient: "linear-gradient(160deg, #0ea5e9 0%, #6366f1 55%, #1e1b4b 100%)",
  },
  {
    id: "r3",
    title: "Morning calm",
    creator: "Noah",
    description: "Soft light and fast, inspiring motion.",
    gradient: "linear-gradient(150deg, #fde68a 0%, #fb7185 55%, #831843 100%)",
  },
];

const PlayBadge = () => (
  <div className="absolute inset-0 flex items-center justify-center">
    <div className="flex h-14 w-14 items-center justify-center rounded-full bg-black/45 ring-1 ring-white/30 backdrop-blur-sm transition group-hover:scale-110">
      <svg viewBox="0 0 24 24" className="ml-0.5 h-6 w-6 fill-current text-white">
        <path d="M8 5v14l11-7z" />
      </svg>
    </div>
  </div>
);

const Videos = () => {
  return (
    <div className="space-y-6">
      <div className="grid gap-6 lg:grid-cols-3">
        {mockedVideos.map((video) => (
          <Link
            key={video.id}
            to={`/v/${video.id}`}
            className="unfrost block overflow-hidden rounded-[2rem] frosted-card shadow-sm transition hover:-translate-y-1 hover:shadow-xl"
          >
            <div
              className="group relative aspect-[4/5] overflow-hidden"
              style={{ backgroundImage: video.gradient }}
            >
              <div className="absolute inset-0 bg-gradient-to-t from-black/55 via-transparent to-transparent" />
              <PlayBadge />
            </div>
            <div className="space-y-4 p-6 text-primary">
              <p className="text-sm uppercase tracking-[0.2em] text-muted">{video.creator}</p>
              <h3 className="text-xl font-semibold text-primary">{video.title}</h3>
              <p className="text-sm leading-6 text-muted">{video.description}</p>
              <span className="block w-full rounded-full bg-accent px-5 py-3 text-center text-sm font-semibold text-accent-text transition group-hover:bg-amber-300">
                Watch
              </span>
            </div>
          </Link>
        ))}
      </div>
    </div>
  );
};

export default Videos;
