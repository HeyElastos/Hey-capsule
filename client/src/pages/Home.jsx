import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { getPosts } from "../api/auth";
import PostCard from "../components/PostCard";
import { ImageIcon } from "../components/icons";
import Landing from "./Landing";

const Home = () => {
  const profile = useMemo(
    () => JSON.parse(localStorage.getItem("profile") || "null"),
    []
  );
  const token = profile?.accessToken;

  const [posts, setPosts] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  useEffect(() => {
    if (!profile) {
      setLoading(false);
      return;
    }
    let active = true;
    (async () => {
      try {
        const data = await getPosts(token);
        if (active) setPosts(data.posts || []);
      } catch (e) {
        if (active) setError("Unable to load feed.");
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => {
      active = false;
    };
  }, [profile]);

  if (!profile) {
    return <Landing />;
  }

  const handleChange = (updated) => {
    setPosts((current) =>
      current.map((p) => (p.id === updated.id ? updated : p))
    );
  };

  const handleDelete = (id) => {
    setPosts((current) => current.filter((p) => p.id !== id));
  };

  return (
    <div className="mx-auto max-w-2xl space-y-6">
      {loading && (
        <div className="space-y-6">
          {[0, 1].map((i) => (
            <div
              key={i}
              className="frosted-card overflow-hidden p-0 animate-fade-in"
              style={{ animationDelay: `${i * 100}ms` }}
            >
              <div className="flex items-center gap-3 p-4">
                <div className="h-10 w-10 rounded-full image-skeleton" />
                <div className="space-y-2">
                  <div className="h-3 w-32 rounded image-skeleton" />
                  <div className="h-2 w-16 rounded image-skeleton" />
                </div>
              </div>
              <div className="aspect-square image-skeleton" />
              <div className="space-y-2 p-4">
                <div className="h-3 w-3/4 rounded image-skeleton" />
                <div className="h-3 w-1/2 rounded image-skeleton" />
              </div>
            </div>
          ))}
        </div>
      )}

      {error && (
        <div className="frosted-card animate-fade-in p-4 text-sm text-red-400">
          {error}
        </div>
      )}

      {!loading && posts.length === 0 && !error && (
        <div className="frosted-card animate-fade-up p-10 text-center">
          <ImageIcon className="mx-auto h-10 w-10 text-accent" />
          <p className="mt-4 text-lg font-semibold text-primary">No posts yet</p>
          <p className="mt-1 text-sm text-muted">
            {token ? "Be the first to share." : "Sign in and start sharing."}
          </p>
          <Link
            to={token ? "/posts" : "/signup"}
            className="unfrost mt-5 inline-block rounded-full bg-accent px-5 py-2.5 text-sm font-semibold text-accent-text transition hover:bg-amber-300"
          >
            {token ? "Create post" : "Get started"}
          </Link>
        </div>
      )}

      {posts.map((post, i) => (
        <div
          key={post.id}
          className="animate-fade-up"
          style={{ animationDelay: `${Math.min(i * 60, 360)}ms` }}
        >
          <PostCard
            post={post}
            currentUser={profile?.user}
            token={token}
            onChange={handleChange}
            onDelete={handleDelete}
          />
        </div>
      ))}
    </div>
  );
};

export default Home;
