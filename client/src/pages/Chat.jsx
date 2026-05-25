import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useProfile } from "../hooks/useProfile";
import { listThreads, getThread, sendMessage } from "../api/chat";
import AddFriendModal from "../components/AddFriendModal";
import { PaperPlaneIcon, ShieldCheckIcon } from "../components/icons";

const POLL_MS = 4000;
const DID_TRUNC = (s) => (s ? s.slice(0, 18) + "…" : "");

const Avatar = ({ name, avatar, size = "h-10 w-10", textSize = "text-sm" }) => {
  const letter = (name || "?").charAt(0).toUpperCase();
  if (avatar) {
    return <img src={avatar} alt={name} className={`${size} rounded-full object-cover`} />;
  }
  return (
    <div
      className={`${size} flex-none rounded-full bg-gradient-to-br from-amber-400 to-pink-400 grid place-items-center font-semibold text-black/80 ${textSize}`}
    >
      {letter}
    </div>
  );
};

const Chat = () => {
  const profile = useProfile();
  const token = profile?.accessToken;
  const myDid = profile?.user?.didKey || "";

  const [threads, setThreads] = useState([]);
  const [activePeerDid, setActivePeerDid] = useState(null);
  const [threadData, setThreadData] = useState(null); // {peer, messages}
  const [draft, setDraft] = useState("");
  const [sending, setSending] = useState(false);
  const [addOpen, setAddOpen] = useState(false);
  const [error, setError] = useState(null);
  const threadEndRef = useRef(null);

  const refreshThreads = useCallback(async () => {
    if (!token) return;
    try {
      const list = await listThreads(token);
      setThreads(list);
    } catch (e) {
      setError(e.response?.data?.message || "Failed to load chats");
    }
  }, [token]);

  const refreshThread = useCallback(async (peerDid) => {
    if (!token || !peerDid) return;
    try {
      const data = await getThread(token, peerDid);
      setThreadData(data);
    } catch (e) {
      setError(e.response?.data?.message || "Failed to load conversation");
    }
  }, [token]);

  // Initial threads load
  useEffect(() => { refreshThreads(); }, [refreshThreads]);

  // Poll threads list every POLL_MS so new messages from other users appear.
  // Phase 3 will swap this for a WS / Carrier subscription.
  useEffect(() => {
    if (!token) return;
    const id = setInterval(refreshThreads, POLL_MS);
    return () => clearInterval(id);
  }, [token, refreshThreads]);

  // When user picks a thread, load it and start polling for new messages.
  useEffect(() => {
    if (!activePeerDid) return;
    refreshThread(activePeerDid);
    const id = setInterval(() => refreshThread(activePeerDid), POLL_MS);
    return () => clearInterval(id);
  }, [activePeerDid, refreshThread]);

  // Auto-scroll to newest on thread update / new message
  useEffect(() => {
    if (threadEndRef.current) {
      threadEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [threadData?.messages?.length, activePeerDid]);

  const handleSend = async (e) => {
    e.preventDefault();
    if (!activePeerDid || !draft.trim() || sending) return;
    setSending(true);
    setError(null);
    const text = draft.trim();
    setDraft("");
    try {
      const newMsg = await sendMessage(token, activePeerDid, text);
      setThreadData((prev) => prev
        ? { ...prev, messages: [...prev.messages, newMsg] }
        : prev);
      refreshThreads();
    } catch (err) {
      setError(err.response?.data?.message || "Failed to send");
      setDraft(text); // restore for retry
    } finally {
      setSending(false);
    }
  };

  const handleAdded = (peer) => {
    setAddOpen(false);
    setActivePeerDid(peer.did);
    setThreadData({ peer: { did: peer.did, name: peer.name, avatar: peer.avatar }, messages: [] });
    refreshThreads();
  };

  const sortedThreads = useMemo(() => threads, [threads]);

  if (!profile) {
    return (
      <div className="mt-24 text-center text-sm text-muted">Sign in to chat.</div>
    );
  }

  if (!myDid) {
    return (
      <div className="mx-auto mt-24 max-w-md rounded-2xl border border-amber-400/30 bg-amber-400/10 p-6 text-center">
        <p className="text-sm text-primary">
          Your account is missing a federation identity. Sign out and back in to
          enable chat — your existing recovery key derives it automatically, no
          re-signup needed.
        </p>
      </div>
    );
  }

  return (
    <div className="mx-auto grid max-w-6xl gap-4 sm:grid-cols-[300px,1fr]">
      {/* ───────────────────────── Sidebar ───────────────────────── */}
      <aside className="frosted-card flex flex-col gap-2 p-3">
        <div className="flex items-center justify-between px-1 pb-1">
          <h2 className="text-xs font-semibold uppercase tracking-wider text-muted">
            Chats
          </h2>
          <button
            type="button"
            onClick={() => setAddOpen(true)}
            className="text-xs font-semibold text-accent hover:underline"
          >
            + Add friend
          </button>
        </div>

        {sortedThreads.length === 0 && (
          <p className="px-2 py-6 text-center text-xs text-muted">
            No conversations yet.
            <br />
            Tap <span className="font-semibold text-accent">+ Add friend</span> to start.
          </p>
        )}

        {sortedThreads.map((t) => (
          <button
            key={t.peer_did}
            type="button"
            onClick={() => setActivePeerDid(t.peer_did)}
            className={`flex items-center gap-3 rounded-2xl p-2 text-left transition ${
              activePeerDid === t.peer_did
                ? "bg-accent/15 ring-1 ring-accent/30"
                : "hover:bg-white/5 dark:hover:bg-white/5"
            }`}
          >
            <Avatar name={t.peer_name} avatar={t.peer_avatar} />
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-semibold text-primary">
                {t.peer_name}
              </div>
              <div className="truncate text-xs text-muted">{t.last_message}</div>
            </div>
          </button>
        ))}

        <div className="mt-auto rounded-2xl border border-white/10 bg-white/5 p-3 text-[10px] font-mono text-muted dark:bg-black/20">
          <div className="mb-1 uppercase tracking-wider">your did:key</div>
          <div className="break-all">{myDid}</div>
        </div>
      </aside>

      {/* ───────────────────────── Thread ───────────────────────── */}
      <section className="frosted-card flex min-h-[60vh] flex-col">
        {!threadData ? (
          <div className="grid flex-1 place-items-center px-6 text-center text-sm text-muted">
            Pick a conversation, or add a friend by their did:key.
          </div>
        ) : (
          <>
            <header className="flex items-center gap-3 border-b border-white/10 px-5 py-4 dark:border-white/10">
              <Avatar name={threadData.peer.name} avatar={threadData.peer.avatar} />
              <div className="min-w-0 flex-1">
                <div className="truncate text-sm font-semibold text-primary">
                  {threadData.peer.name}
                </div>
                <div
                  className="truncate font-mono text-[10px] text-muted"
                  title={threadData.peer.did}
                >
                  {DID_TRUNC(threadData.peer.did)}
                </div>
              </div>
              <span
                className="flex items-center gap-1 rounded-full bg-amber-400/15 px-2 py-0.5 text-[10px] font-semibold text-amber-700 dark:text-amber-300"
                title="Phase 2: local-mode. Phase 3 will add end-to-end signatures."
              >
                <ShieldCheckIcon className="h-3 w-3" />
                local
              </span>
            </header>

            <div className="flex-1 space-y-2 overflow-y-auto px-4 py-4">
              {threadData.messages.length === 0 ? (
                <p className="mt-8 text-center text-xs text-muted">
                  No messages yet. Say hi.
                </p>
              ) : (
                threadData.messages.map((m) => {
                  const mine = m.sender_did === myDid;
                  return (
                    <div
                      key={m.id}
                      className={`flex ${mine ? "justify-end" : "justify-start"}`}
                    >
                      <div
                        className={`max-w-[72%] rounded-2xl px-4 py-2 text-sm leading-snug ${
                          mine
                            ? "rounded-br-md bg-amber-400/20 text-amber-900 ring-1 ring-amber-400/40 dark:text-amber-100"
                            : "rounded-bl-md bg-white/70 text-primary ring-1 ring-black/5 dark:bg-white/10 dark:ring-white/10"
                        }`}
                      >
                        <div className="whitespace-pre-wrap break-words">{m.content}</div>
                        <div className="mt-1 text-right text-[10px] opacity-70">
                          {new Date(m.ts).toLocaleTimeString([], {
                            hour: "2-digit",
                            minute: "2-digit",
                          })}
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
              <div ref={threadEndRef} />
            </div>

            {error && (
              <p className="mx-4 mb-2 animate-fade-in text-xs text-red-500 dark:text-red-400">
                {error}
              </p>
            )}

            <form
              onSubmit={handleSend}
              className="flex items-center gap-2 border-t border-white/10 px-3 py-3 dark:border-white/10"
            >
              <input
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
                placeholder="Type a message…"
                disabled={sending}
                className="frosted-input flex-1 text-sm"
                maxLength={2000}
              />
              <button
                type="submit"
                disabled={sending || !draft.trim()}
                aria-label="Send"
                className="grid h-10 w-10 flex-none place-items-center rounded-full bg-accent text-accent-text transition hover:bg-amber-300 disabled:cursor-not-allowed disabled:opacity-50"
              >
                <PaperPlaneIcon className="h-4 w-4" />
              </button>
            </form>
          </>
        )}
      </section>

      {addOpen && (
        <AddFriendModal
          token={token}
          onClose={() => setAddOpen(false)}
          onAdded={handleAdded}
        />
      )}
    </div>
  );
};

export default Chat;
