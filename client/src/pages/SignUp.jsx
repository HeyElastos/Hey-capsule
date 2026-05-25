import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { signUp } from "../api/auth";
import { setProfile } from "../hooks/useProfile";

const SignUp = () => {
  const navigate = useNavigate();
  const [name, setName] = useState("");
  const [key, setKey] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  const handleSubmit = async (event) => {
    event.preventDefault();
    setError(null);
    setLoading(true);

    try {
      const data = await signUp({ name });
      const profile = {
        user: data.user,
        accessToken: data.accessToken,
        refreshToken: data.refreshToken,
      };
      setProfile(profile);
      setKey(data.authKey);
    } catch (err) {
      setError(err.response?.data?.message || "Unable to create account.");
    } finally {
      setLoading(false);
    }
  };

  const handleCopy = () => {
    if (key) {
      navigator.clipboard.writeText(key);
    }
  };

  return (
    <div className="mx-auto max-w-2xl rounded-[2rem] frosted-card p-8 shadow-2xl shadow-slate-950/30 text-primary">
      <div className="grid gap-8 lg:grid-cols-[1.1fr_0.9fr] lg:items-center">
        <div className="space-y-4">
          <p className="text-sm uppercase tracking-[0.3em] text-accent">Fast signup</p>
          <h2 className="text-4xl font-bold text-primary">Create your Hey account</h2>
          <p className="text-muted">
            Choose a display name and get a secure login key instantly. No email, no password, just one private key.
          </p>
        </div>
        <div className="rounded-[1.75rem] frosted-card p-6 text-primary shadow-xl">
          <p className="text-sm uppercase tracking-[0.2em] text-accent">Key-based access</p>
          <p className="mt-4 text-lg leading-7">
            Your key is your only credential. Keep it safe and use it to access your profile, feed, and videos.
          </p>
        </div>
      </div>

      {key ? (
        <div className="mt-8 space-y-4">
          <div className="rounded-3xl frosted-card p-6 shadow-sm text-primary">
            <p className="text-sm text-muted">Your Hey key</p>
            <p className="mt-3 break-all rounded-3xl frosted-card p-4 font-mono text-primary shadow-sm">
              {key}
            </p>
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <button
              onClick={handleCopy}
              className="rounded-full bg-blue-600 px-5 py-3 text-white hover:bg-blue-700"
            >
              Copy key
            </button>
            <button
              onClick={() => navigate("/welcome")}
              className="rounded-full border border-surface-border px-5 py-3 text-primary hover:bg-surface-soft"
            >
              Go home
            </button>
          </div>
        </div>
      ) : (
        <form onSubmit={handleSubmit} className="mt-8 space-y-6">
          <label className="block text-sm font-medium text-primary">Display name</label>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="Alice"
            className="frosted-input"
          />
          {error && <p className="text-sm text-red-600">{error}</p>}
          <button
            type="submit"
            disabled={loading}
            className="w-full rounded-full bg-accent px-6 py-4 text-accent-text shadow-lg shadow-slate-900/10 transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
          >
            {loading ? "Creating account..." : "Create account"}
          </button>
        </form>
      )}
    </div>
  );
};

export default SignUp;
