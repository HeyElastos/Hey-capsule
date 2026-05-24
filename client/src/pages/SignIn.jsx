import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { signIn } from "../api/auth";

const SignIn = () => {
  const navigate = useNavigate();
  const [authKey, setAuthKey] = useState("");
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (event) => {
    event.preventDefault();
    setError(null);
    setLoading(true);

    try {
      const data = await signIn({ authKey });
      const profile = {
        user: data.user,
        accessToken: data.accessToken,
        refreshToken: data.refreshToken,
      };
      localStorage.setItem("profile", JSON.stringify(profile));
      navigate("/");
    } catch (err) {
      setError(err.response?.data?.message || "Unable to sign in.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="mx-auto max-w-2xl rounded-[2rem] frosted-card p-8 shadow-2xl shadow-slate-950/30 text-primary">
      <div className="grid gap-8 lg:grid-cols-[1.1fr_0.9fr] lg:items-center">
        <div className="space-y-4">
          <p className="text-sm uppercase tracking-[0.3em] text-accent">Key login</p>
          <h2 className="text-4xl font-bold text-primary">Access your Hey account</h2>
          <p className="text-muted">
            Paste your unique profile key to sign in. No email, no password, just one secure key.
          </p>
        </div>

        <div className="rounded-[1.75rem] frosted-card p-6 text-primary shadow-xl">
          <p className="text-sm uppercase tracking-[0.2em] text-accent">Instant social login</p>
          <p className="mt-4 text-lg leading-7">
            Your key is your identity. Keep it safe and use it to open your feed, profile, and videos.
          </p>
        </div>
      </div>

      <form onSubmit={handleSubmit} className="mt-8 space-y-6">
        <label className="block text-sm font-medium text-primary">Hey key</label>
        <textarea
          value={authKey}
          onChange={(event) => setAuthKey(event.target.value)}
          rows={5}
          placeholder="Paste your key here"
          className="frosted-input"
        />
        {error && <p className="text-sm text-red-600">{error}</p>}
        <button
          type="submit"
          disabled={loading}
          className="w-full rounded-full bg-accent px-6 py-4 text-accent-text shadow-lg shadow-slate-900/10 transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-60"
        >
          {loading ? "Signing in..." : "Sign in"}
        </button>
      </form>
    </div>
  );
};

export default SignIn;
