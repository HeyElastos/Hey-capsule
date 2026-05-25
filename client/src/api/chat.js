import axios from "axios";

const API = axios.create({ baseURL: "/api" });
const auth = (token) => ({ Authorization: `Bearer ${token}` });

export const listThreads = async (token) => {
  const { data } = await API.get("/chat/threads", { headers: auth(token) });
  return data.threads || [];
};

export const getThread = async (token, peerDid, opts = {}) => {
  const params = new URLSearchParams();
  if (opts.before) params.set("before", String(opts.before));
  if (opts.limit) params.set("limit", String(opts.limit));
  const qs = params.toString() ? `?${params}` : "";
  const { data } = await API.get(`/chat/threads/${encodeURIComponent(peerDid)}${qs}`, {
    headers: auth(token),
  });
  return data;
};

export const sendMessage = async (token, peerDid, content) => {
  const { data } = await API.post(
    `/chat/threads/${encodeURIComponent(peerDid)}/messages`,
    { content },
    { headers: auth(token) }
  );
  return data.message;
};

export const followPeer = async (token, did) => {
  const { data } = await API.post(
    "/chat/follow",
    { did },
    { headers: auth(token) }
  );
  return data;
};
