//! Platform abstraction for the shared chat engine.
//!
//! `hey-core` runs in two worlds:
//!   * the browser (wasm32) — inside hey-social / hey-chat. Time = `Date.now`,
//!     sleep = `setTimeout`, HTTP = `fetch`, storage = the runtime's HTTP
//!     storage route, logging = `console`.
//!   * a native CLI (`hey-chat-cli`, the cross-runtime DM diagnostic) — Time =
//!     `SystemTime`, sleep = `thread::sleep`, HTTP = a tiny loopback TcpStream
//!     client (the runtime API is plaintext on 127.0.0.1, so no TLS), storage =
//!     local JSON files, logging = `eprintln`.
//!
//! ONLY these leaf primitives diverge. Every byte of protocol logic
//! (dms.rs / outbox.rs / the `peer`/`identity`/`content` provider wrappers)
//! is shared and identical across both, so the CLI exercises the EXACT invite/
//! handshake code path the apps run — which is the whole point: trace where a
//! real cross-runtime invite goes wrong without a browser.

#[cfg(target_arch = "wasm32")]
mod imp {
    pub fn now_ms() -> i64 {
        js_sys::Date::now() as i64
    }

    pub async fn sleep_ms(ms: i32) {
        let win = web_sys::window().expect("no window");
        let promise = js_sys::Promise::new(&mut |resolve, _reject| {
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        });
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
    }

    pub fn warn(s: &str) {
        web_sys::console::warn_1(&wasm_bindgen::JsValue::from_str(s));
    }

    pub fn debug(s: &str) {
        web_sys::console::debug_1(&wasm_bindgen::JsValue::from_str(s));
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    thread_local! {
        static BASE: RefCell<String> = RefCell::new("http://127.0.0.1:3000".to_string());
        static BEARER: RefCell<String> = RefCell::new(String::new());
        static STORE: RefCell<PathBuf> = RefCell::new(PathBuf::from("/tmp/hey-cli"));
    }

    // Optional IN-PROCESS provider dispatch. When set (the mobile mini-runtime registers it),
    // `http()` routes provider calls straight to the in-process handlers — NO TCP socket is opened
    // at all, so there is no loopback surface for a co-installed app to reach. The CLI / host dev
    // harness never set it and keep using the loopback TcpStream client below, unchanged.
    type Dispatch = Box<dyn Fn(&str, &str, Option<&str>) -> Result<(u16, String), String> + Send + Sync>;
    static DISPATCH: OnceLock<Dispatch> = OnceLock::new();
    pub fn set_dispatch<F>(f: F)
    where
        F: Fn(&str, &str, Option<&str>) -> Result<(u16, String), String> + Send + Sync + 'static,
    {
        let _ = DISPATCH.set(Box::new(f));
    }
    /// True once an in-process dispatcher is installed (mobile readiness signal).
    pub fn dispatch_ready() -> bool {
        DISPATCH.get().is_some()
    }

    /// Configure the native runtime endpoint, bearer token, and storage root.
    /// Called once from the CLI's `main` before any engine call.
    pub fn set_base(b: &str) {
        BASE.with(|x| *x.borrow_mut() = b.trim_end_matches('/').to_string());
    }
    pub fn set_bearer(b: &str) {
        BEARER.with(|x| *x.borrow_mut() = b.to_string());
    }
    pub fn set_store(dir: &str) {
        STORE.with(|x| *x.borrow_mut() = PathBuf::from(dir));
    }
    pub fn base_url() -> String {
        BASE.with(|x| x.borrow().clone())
    }
    fn bearer() -> String {
        BEARER.with(|x| x.borrow().clone())
    }
    fn store_root() -> PathBuf {
        STORE.with(|x| x.borrow().clone())
    }

    // ── At-rest encryption key (storage DEK) ─────────────────────────────
    //
    // PROCESS-GLOBAL (not thread-local like STORE): the mobile runtime installs
    // it ONCE after the user unlocks, and every storage thread (receiver loops,
    // JNI calls) must see the same key. The DEK is wrapped by a hardware
    // (StrongBox/TEE) Keystore key on the Kotlin side; when it is set, every
    // `file_write` seals and every `file_read` opens transparently. When it is
    // NOT set (the CLI / host dev harness), storage stays plaintext exactly as
    // before — these helpers are a no-op there.
    // Two-key storage split: the DEK is now CLEARABLE (Mutex, not OnceLock) so the
    // app can LOCK storage — drop the plaintext DEK from memory on app-lock so an
    // in-process attacker / grabbed-unlocked phone can't read the seed, ratchet
    // keys, or conversations. The carrier keeps RECEIVING + buffering sealed wires
    // (broker.json needs no DEK); the receiver defers processing until the DEK is
    // re-installed on biometric unlock. STORAGE_LOCKED distinguishes "locked"
    // (vault user, DEK cleared) from "no DEK ever" (CLI / plaintext storage).
    static AT_REST_KEY: Mutex<Option<[u8; 32]>> = Mutex::new(None);
    static STORAGE_LOCKED: AtomicBool = AtomicBool::new(false);
    // Headless-vault: a vault-ON device cold-starts with the storage DEK present
    // (no-auth, hardware-wrapped → reads contacts/profile to JOIN topics + mesh)
    // but the SEED sealed in the biometric vault. IDENTITY_SEALED marks that
    // distinct state: at-rest IS readable, but DM/ratchet DECRYPT can't run (the
    // seed-derived keys are absent), so the receiver buffers and drains on unlock.
    // Orthogonal to STORAGE_LOCKED (DEK gone): EITHER one means "don't consume".
    static IDENTITY_SEALED: AtomicBool = AtomicBool::new(false);

    /// Install the 32-byte storage DEK (released from the hardware-wrapped KEK
    /// after unlock). Clears the locked flag so the receiver resumes + drains.
    pub fn set_at_rest_key(key: [u8; 32]) {
        if let Ok(mut g) = AT_REST_KEY.lock() {
            *g = Some(key);
        }
        STORAGE_LOCKED.store(false, Ordering::SeqCst);
    }
    /// LOCK storage: zeroize + drop the DEK from memory and mark locked. Background
    /// receipt keeps buffering; processing + sensitive reads resume on re-install.
    pub fn lock_storage() {
        if let Ok(mut g) = AT_REST_KEY.lock() {
            if let Some(mut k) = g.take() {
                k.fill(0);
            }
        }
        STORAGE_LOCKED.store(true, Ordering::SeqCst);
    }
    /// True while storage is locked (vault user, DEK intentionally cleared). The
    /// receiver skips processing so buffered wires stay sealed until unlock.
    pub fn storage_locked() -> bool {
        STORAGE_LOCKED.load(Ordering::SeqCst)
    }
    /// Mark whether the seed is currently sealed (vault-ON headless boot: true
    /// until `hey_unlock`). Set false once the seed-backed identity is installed.
    pub fn set_identity_sealed(sealed: bool) {
        IDENTITY_SEALED.store(sealed, Ordering::SeqCst);
    }
    /// True when the seed is sealed (headless boot, pre-unlock). DM/ratchet decrypt
    /// can't run; the receiver buffers inbound wires and drains them on unlock.
    pub fn identity_sealed() -> bool {
        IDENTITY_SEALED.load(Ordering::SeqCst)
    }
    /// Unified "can't decrypt/process right now" gate: storage locked (no DEK) OR
    /// seed sealed (headless). Either way the receiver JOINS + buffers but defers
    /// consume. False = fully unlocked, safe to decrypt + flush.
    pub fn processing_deferred() -> bool {
        storage_locked() || identity_sealed()
    }
    /// True when a storage DEK is currently installed (files encrypted at rest).
    pub fn at_rest_active() -> bool {
        AT_REST_KEY.lock().map(|g| g.is_some()).unwrap_or(false)
    }
    fn at_rest_key() -> Option<[u8; 32]> {
        AT_REST_KEY.lock().ok().and_then(|g| *g)
    }
    /// Seal bytes with the installed DEK, or `None` if no key is set. Lets code
    /// outside this module (the runtime's identity.rs) encrypt files that don't
    /// live under the storage root with the same key.
    pub fn seal_with_at_rest_key(plaintext: &[u8]) -> Option<Vec<u8>> {
        at_rest_key().map(|k| crate::crypto::seal_at_rest(&k, plaintext))
    }
    /// Open a blob with the installed DEK. `None` if no key, no magic, or the tag
    /// fails. Pair with `crate::crypto::is_at_rest` to detect legacy plaintext.
    pub fn open_with_at_rest_key(blob: &[u8]) -> Option<Vec<u8>> {
        let k = at_rest_key()?;
        crate::crypto::open_at_rest(&k, blob)
    }

    pub fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    pub async fn sleep_ms(ms: i32) {
        if ms > 0 {
            std::thread::sleep(Duration::from_millis(ms as u64));
        }
    }

    pub fn warn(s: &str) {
        eprintln!("[warn] {s}");
    }

    pub fn debug(s: &str) {
        if std::env::var("HEY_DEBUG").is_ok() {
            eprintln!("[debug] {s}");
        }
    }

    // ── Loopback HTTP/1.1 client (plaintext, 127.0.0.1 only) ─────────────
    //
    // Everything the CLI talks to is the LOCAL runtime, so no TLS. We send
    // `Connection: close` and read to EOF, decoding `Transfer-Encoding:
    // chunked` if present (hyper sometimes chunks). Returns (status, body).

    fn parse_url(url: &str) -> Result<(String, u16, String), String> {
        let rest = url
            .strip_prefix("http://")
            .ok_or_else(|| format!("only http:// supported (loopback): {url}"))?;
        let slash = rest.find('/').unwrap_or(rest.len());
        let authority = &rest[..slash];
        let path = if slash < rest.len() { &rest[slash..] } else { "/" };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), p.parse::<u16>().map_err(|_| "bad port")?),
            None => (authority.to_string(), 80),
        };
        Ok((host, port, path.to_string()))
    }

    fn decode_body(raw: &[u8]) -> Result<(u16, String), String> {
        // Split headers / body on the first CRLFCRLF.
        let sep = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("malformed HTTP response (no header terminator)")?;
        let head = String::from_utf8_lossy(&raw[..sep]);
        let body_bytes = &raw[sep + 4..];
        let mut lines = head.split("\r\n");
        let status_line = lines.next().unwrap_or("");
        let status: u16 = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("bad status line: {status_line}"))?;
        let chunked = lines.any(|l| {
            let l = l.to_ascii_lowercase();
            l.starts_with("transfer-encoding:") && l.contains("chunked")
        });
        let body = if chunked {
            dechunk(body_bytes)
        } else {
            String::from_utf8_lossy(body_bytes).to_string()
        };
        Ok((status, body))
    }

    fn dechunk(mut b: &[u8]) -> String {
        let mut out = Vec::new();
        loop {
            let Some(nl) = b.windows(2).position(|w| w == b"\r\n") else {
                break;
            };
            let size_str = String::from_utf8_lossy(&b[..nl]);
            let size = usize::from_str_radix(size_str.trim(), 16).unwrap_or(0);
            if size == 0 {
                break;
            }
            let start = nl + 2;
            let end = (start + size).min(b.len());
            out.extend_from_slice(&b[start..end]);
            // Skip the chunk + its trailing CRLF.
            b = if end + 2 <= b.len() { &b[end + 2..] } else { &[] };
        }
        String::from_utf8_lossy(&out).to_string()
    }

    pub fn http(method: &str, url: &str, body: Option<&str>) -> Result<(u16, String), String> {
        // In-process dispatch (mobile): handle the call in-process, no socket. Falls through to the
        // loopback TcpStream client only when no dispatcher is installed (CLI / host dev harness).
        if let Some(d) = DISPATCH.get() {
            return d(method, url, body);
        }
        let (host, port, path) = parse_url(url)?;
        let mut stream = TcpStream::connect((host.as_str(), port))
            .map_err(|e| format!("connect {host}:{port}: {e}"))?;
        // 90s: a content/fetch for a freshly-pinned CID can block on bitswap
        // discovery longer than 25s on the first hit (the browser uses fetch()
        // with no such cap, so this only affects the native CLI diagnostic —
        // a short cap made it false-report EAGAIN where the app would wait+win).
        stream.set_read_timeout(Some(Duration::from_secs(90))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(90))).ok();
        let b = body.unwrap_or("");
        let tok = bearer();
        let mut req = String::new();
        req.push_str(&format!("{method} {path} HTTP/1.1\r\n"));
        req.push_str(&format!("Host: {host}\r\n"));
        req.push_str("Connection: close\r\n");
        req.push_str("Content-Type: application/json\r\n");
        if !tok.is_empty() {
            req.push_str(&format!("Authorization: Bearer {tok}\r\n"));
        }
        req.push_str(&format!("Content-Length: {}\r\n\r\n", b.len()));
        req.push_str(b);
        stream
            .write_all(req.as_bytes())
            .map_err(|e| format!("write: {e}"))?;
        stream.flush().ok();
        let mut raw = Vec::new();
        stream
            .read_to_end(&mut raw)
            .map_err(|e| format!("read: {e}"))?;
        decode_body(&raw)
    }

    // ── File-backed storage + KV ─────────────────────────────────────────
    //
    // Mirrors the runtime's per-capsule storage namespace as a directory tree
    // under the configured store root. `suffix` is e.g. "HeyChat/dm/outbox.json".

    fn safe_path(suffix: &str) -> PathBuf {
        let mut p = store_root();
        for seg in suffix.split('/').filter(|s| !s.is_empty() && *s != "." && *s != "..") {
            p.push(seg);
        }
        p
    }

    /// Read a stored JSON document. `None` == the file does not exist (404).
    /// When a storage DEK is installed (mobile), files are decrypted in place;
    /// a pre-encryption LEGACY plaintext file is still read (and re-encrypted on
    /// its next write), while an at-rest blob that fails to open (wrong key /
    /// tamper) is treated as absent rather than returned as garbage.
    pub fn file_read(suffix: &str) -> Option<String> {
        let raw = std::fs::read(safe_path(suffix)).ok()?;
        if at_rest_active() {
            if crate::crypto::is_at_rest(&raw) {
                match open_with_at_rest_key(&raw) {
                    Some(pt) => String::from_utf8(pt).ok(),
                    None => {
                        warn(&format!("at-rest decrypt failed for {suffix} (corrupt or wrong key)"));
                        None
                    }
                }
            } else {
                // Legacy plaintext from before encryption was enabled — migrate
                // transparently (the next file_write seals it).
                String::from_utf8(raw).ok()
            }
        } else {
            String::from_utf8(raw).ok()
        }
    }

    /// Per-write sequence for collision-free atomic-write temp names (see file_write).
    static TMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    pub fn file_write(suffix: &str, content: &str) -> Result<(), String> {
        let p = safe_path(suffix);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir {parent:?}: {e}"))?;
        }
        // ATOMIC write — this is the REAL mobile persistence path for the sealed contacts / ratchet /
        // groups / posts / feed-key blobs. std::fs::write is truncate-then-write, so a crash or
        // interleaved write mid-way leaves a TORN blob — and a torn sealed-at-rest blob is
        // undecryptable = the WHOLE file is lost. Write to a UNIQUE sibling .heytmp<n> then rename
        // over the target (rename(2) is atomic on the same filesystem; the live file is untouched
        // until the swap). A per-write sequence makes the tmp name collision-free even for two
        // concurrent unlocked writers to the same key (last rename wins = lost update, never a torn
        // file). On failure the tmp is cleaned and the existing file is left intact.
        let seq = TMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = p.with_extension(format!("heytmp{seq}"));
        let w = match seal_with_at_rest_key(content.as_bytes()) {
            Some(blob) => std::fs::write(&tmp, blob),
            None => std::fs::write(&tmp, content),
        };
        w.map_err(|e| format!("write {tmp:?}: {e}"))?;
        std::fs::rename(&tmp, &p).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("rename {p:?}: {e}")
        })
    }

    pub fn file_remove(suffix: &str) -> Result<(), String> {
        let p = safe_path(suffix);
        match std::fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("remove {p:?}: {e}")),
        }
    }

    /// Key/value store (the session record). Backed by `<store>/kv/<key>.json`.
    pub fn kv_get(key: &str) -> Option<String> {
        file_read(&format!("kv/{key}.json"))
    }
    pub fn kv_set(key: &str, val: &str) {
        let _ = file_write(&format!("kv/{key}.json"), val);
    }
    pub fn kv_del(key: &str) {
        let _ = file_remove(&format!("kv/{key}.json"));
    }
}

pub use imp::*;
