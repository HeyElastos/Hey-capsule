// Hey shell — launcher logic.
// Pure vanilla + the global Tauri bridge (app.withGlobalTauri = true), so there is
// no JS build step. We only ever call our own Rust commands; once `connect` runs,
// the Rust side navigates this same webview to the remote runtime origin and this
// page is gone until the user comes back (Android back / re-launch).

const invoke = window.__TAURI__?.core?.invoke;

const $ = (id) => document.getElementById(id);
const hostEl = $("host");
const tokenEl = $("token");
const ntfyUrlEl = $("ntfy_url");
const ntfyTopicEl = $("ntfy_topic");
const statusEl = $("status");
const connectBtn = $("connect");
const scanBtn = $("scan");
const cancelScanBtn = $("cancel-scan");

function setStatus(msg, kind = "") {
  statusEl.textContent = msg || "";
  statusEl.className = "status" + (kind ? " " + kind : "");
}

function selectedApp() {
  const r = document.querySelector('input[name="app"]:checked');
  return r ? r.value : "hey-social";
}

function setApp(value) {
  const r = document.querySelector(`input[name="app"][value="${value}"]`);
  if (r) r.checked = true;
}

// Prefill from (1) a deep link / URL query that opened the launcher, then
// (2) the persisted config from the last successful connect.
async function prefill() {
  const q = new URLSearchParams(window.location.search);
  let prefilled = false;
  if (q.get("host")) { hostEl.value = q.get("host"); prefilled = true; }
  if (q.get("token") || q.get("home_token")) {
    tokenEl.value = q.get("token") || q.get("home_token"); prefilled = true;
  }
  if (q.get("app")) { setApp(q.get("app")); prefilled = true; }

  if (invoke) {
    try {
      const cfg = await invoke("load_config");
      if (cfg) {
        if (!hostEl.value && cfg.host) hostEl.value = cfg.host;
        if (!tokenEl.value && cfg.token) tokenEl.value = cfg.token;
        if (cfg.app) setApp(cfg.app);
        if (cfg.ntfy_url) ntfyUrlEl.value = cfg.ntfy_url;
        if (cfg.ntfy_topic) ntfyTopicEl.value = cfg.ntfy_topic;
      }
    } catch (e) {
      console.warn("load_config failed", e);
    }
    listenForPush();
  } else {
    setStatus("Running outside Tauri — connect is disabled.", "err");
  }

  // If a deep link carried everything, offer one-tap connect.
  if (prefilled && hostEl.value && tokenEl.value && invoke) {
    setStatus("Tap Connect to open.", "ok");
  }
}

async function connect() {
  const host = hostEl.value.trim();
  const token = tokenEl.value.trim();
  const app = selectedApp();

  if (!host) { setStatus("Enter your runtime host.", "err"); hostEl.focus(); return; }
  if (!invoke) { setStatus("Tauri bridge unavailable.", "err"); return; }

  connectBtn.disabled = true;
  setStatus("Connecting…");
  try {
    await invoke("connect", {
      host,
      token,
      appName: app,
      ntfyUrl: ntfyUrlEl.value.trim(),
      ntfyTopic: ntfyTopicEl.value.trim(),
    });
    // On success the webview is already navigating away; nothing else to do.
  } catch (e) {
    connectBtn.disabled = false;
    setStatus(String(e), "err");
  }
}

// ── QR scan (camera) ─────────────────────────────────────────────────────
// Scan the "Link phone" QR shown by desktop hey-social. The QR encodes
// heyapp://connect?host=…&app=…&token=… — we parse it, fill the fields, and
// connect automatically. The scanner makes the webview transparent, so we add
// a `scanning` class to clear the UI and show a Cancel button over the camera.
const bscan = (cmd, args) =>
  invoke ? invoke(`plugin:barcode-scanner|${cmd}`, args || {}) : Promise.reject("no bridge");

async function scanQr() {
  if (!invoke) { setStatus("Scanner unavailable.", "err"); return; }
  try {
    let perm = await bscan("check_permissions").catch(() => null);
    if (!perm || perm.camera !== "granted") {
      perm = await bscan("request_permissions").catch(() => null);
    }
    if (perm && perm.camera === "denied") {
      setStatus("Camera permission denied.", "err");
      return;
    }
    document.documentElement.classList.add("scanning");
    setStatus("Point at the QR on your desktop…");
    const res = await bscan("scan", { windowed: false, formats: ["QR_CODE"] });
    document.documentElement.classList.remove("scanning");
    applyScanned((res && res.content) || "");
  } catch (e) {
    document.documentElement.classList.remove("scanning");
    setStatus(String(e), "err");
  }
}

async function cancelScan() {
  try { await bscan("cancel"); } catch (_) {}
  document.documentElement.classList.remove("scanning");
  setStatus("");
}

function applyScanned(content) {
  let host, token, app;
  try {
    const u = new URL(content);
    host = u.searchParams.get("host");
    token = u.searchParams.get("token");
    app = u.searchParams.get("app") || "hey-social";
  } catch (_) { /* not a URL */ }

  if (host && token) {
    hostEl.value = host;
    tokenEl.value = token;
    setApp(app);
    setStatus("Scanned — connecting…", "ok");
    connect();
  } else {
    setStatus("That QR isn't a Hey link.", "err");
  }
}

// A push arrived (ntfy → native notification → this event). The launcher just
// surfaces it; once navigated to a capsule, the capsule listens to refresh.
function listenForPush() {
  const listen = window.__TAURI__?.event?.listen;
  if (!listen) return;
  listen("hey://push", (e) => {
    setStatus("🔔 " + (e.payload || "new message"), "ok");
  }).catch((err) => console.warn("push listen failed", err));
}

connectBtn.addEventListener("click", connect);
scanBtn.addEventListener("click", scanQr);
cancelScanBtn.addEventListener("click", cancelScan);
window.addEventListener("DOMContentLoaded", prefill);
