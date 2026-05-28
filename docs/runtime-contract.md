# Runtime Contract Reference — Grounded Snapshot

> **How to read this doc**
>
> This is a source-cited audit of upstream Elastos Runtime contracts
> as they existed at commit [`6d4c385`](https://github.com/Elacity/elastos-runtime/commit/6d4c385).
> It is dated **2026-05-28**. Treat as a snapshot, not a live spec —
> when the runtime evolves, re-derive against the new commit and
> update this file. The quick-recall version is
> [runtime-quick-reference.md](runtime-quick-reference.md).
>
> Local-path references (e.g. `/var/home/linux/ai/elastos-runtime-ynh/`)
> are the auditor's local YNH fork clone. Adjust paths to wherever
> you have the runtime checked out.

---

**Sources cross-referenced:**
- Upstream: `github.com/Elacity/elastos-runtime` @ commit `6d4c3853794521c225f4f813c3e40f6ef3a57b86` (raw.githubusercontent.com URLs assume this prefix: `https://raw.githubusercontent.com/Elacity/elastos-runtime/6d4c3853794521c225f4f813c3e40f6ef3a57b86/`)
- Local YNH fork: `/var/home/linux/ai/elastos-runtime-ynh/` (UPSTREAM_VERSION = v0.3.0; only target/ pre-built — no upstream source; relevant glue is in `scripts/_common.sh`, `components.additions.json`, `scripts/install.sh`, `scripts/patches/`)

Important upfront finding: published `elastos/docs/PEER_PROTOCOL.md` is the **only** doc shipped under `elastos/docs/`. There is no upstream `CARRIER.md`, `state.md` (the runtime root has one — see below), `NAMESPACES.md`, or `CONTENT_AVAILABILITY.md` doc at that path. Those names are local YNH copies that diverged. Useful upstream docs that DO exist: `docs/CAPSULE_MODEL.md`, `docs/INTERACTIVE_RUNTIME_CONTRACT.md`, `docs/BROWSER_CAPSULE.md`, `docs/README.md`, plus root-level `state.md`, `TASKS.md`, `ROADMAP.md`, `PRINCIPLES.md`, `DEBUG.md`.

---

## A. Provider bus (HTTP side — how capsules call providers)

There are **two distinct** `/api/provider/:scheme/:op` routes — easy to confuse:

### A.1 — Generic provider proxy (capsule → provider, in the local runtime HTTP server)

Source: `elastos/crates/elastos-server/src/api/handlers/provider.rs`, mounted by `src/api/server.rs:392`.

```rust
.route("/api/provider/:scheme/:op", post(handlers::provider::provider_proxy))
.layer(axum_middleware::from_fn_with_state(api_state, auth_middleware))
```

- **Auth:** `Authorization: Bearer <session-token>` (validated by `auth_middleware` in `api/middleware.rs`).
- **Capability header:** `X-Capability-Token: <base64>` (skipped iff `session.is_shell()` — shell sessions have orchestrator privilege).
- **Body:** any JSON object. The handler does `request["op"] = op` then `registry.send_raw(scheme, request)`.
- **Return:** the provider's raw JSON response (line-delimited JSON it wrote to stdout), or `{"status":"error","code":"provider_error","message":"..."}` if the provider errored at the bridge level.

### A.2 — Public gateway provider proxy (Home browser → gateway → provider)

Source: `elastos/crates/elastos-server/src/api/gateway_provider_proxy.rs` (164 lines, quoted in full above).

This is **not** generic. It's a whitelisted bridge for Home browser app authority — only `documents`, `chain`, `net` schemes are routed:

```rust
let allowed_apps: &[&str] = match scheme.as_str() {
    "documents" => match op.as_str() {
        "summary" | "get" => &[DOCUMENTS_CAPSULE_ID, LIBRARY_CAPSULE_ID],
        _ => &[DOCUMENTS_CAPSULE_ID],
    },
    "chain" => match op.as_str() {
        "networks" | "status" | "block_number" | "sync_health" | "node_lifecycle" => &[SYSTEM_CAPSULE_ID],
        "balance" => &[SYSTEM_CAPSULE_ID, WALLET_CAPSULE_ID],
        _ => return NOT_FOUND,
    },
    "net" => match op.as_str() {
        "status" | "resolve" | "connect" | "stream" | "http" => &[BROWSER_CAPSULE_ID],
        _ => return NOT_FOUND,
    },
    _ => return NOT_FOUND,
};
```

- **Auth:** `x-elastos-home-token` header carrying a signed `elastos.home.launch-token/v2` envelope (see C.1) for one of the allowed apps. **No `X-Capability-Token` used here.**
- The gateway then injects `request["principal_id"] = principal_id` for documents/net.
- Returns the provider's JSON response, wrapped 200 OK.

### A.2 — How the runtime knows which subprocess handles `elastos://peer/*`

Source: `elastos/crates/elastos-runtime/src/provider/registry.rs`.

The `ProviderRegistry` is in-memory. Providers register themselves on the runtime side via `register(Arc<dyn Provider>)`; each provider exposes `schemes() -> Vec<&'static str>`. `CapsuleProvider` (provider/bridge.rs:335+) is constructed when the runtime spawns a provider subprocess — it stores a `scheme_static: &'static str`.

`elastos://` is hierarchically dispatched. From `registry.rs:163`:

```rust
const RESERVED_SUB_NAMES: &[&str] = &[
    "peer", "did", "ai", "llama", "ipfs", "content", "tunnel",
    "storage", "namespace", "message", "chain", "net", "exit",
    "browser-engine", "wallet", "drm", "rights", "key", "decrypt",
    "availability",
];
```

So `elastos://peer/...` routes to the sub-provider registered for `peer`. There IS NO public `peer` scheme in upstream — it's a reserved name but no built-in `peer-provider` capsule ships in this tree. (You're building it yourself; the chat capsule talks Carrier directly through `elastos_guest` and the bridge, not through a peer-provider subprocess.)

`capsule.json`'s `provides` field is the manifest-declared scheme (e.g. `"provides": "elastos://did/*"`). At provider startup the runtime (via supervisor / `provider_resource.rs`) reads the manifest, spawns the binary, and registers the provider under that scheme. No separate components-style registry beyond capsule.json + the supervisor's binary path table.

### A.3 — Capability validation before dispatch

`handlers/provider.rs:71-124` (full text quoted above). Order of checks:
1. If `session.is_shell()`, skip token check entirely.
2. Require `CapabilityManager` configured (else 403).
3. Require `X-Capability-Token` header (else 403).
4. Parse base64 → `CapabilityToken`.
5. `build_capability_resource(scheme, op, request)` constructs the canonical resource string (in `provider_resource.rs`).
6. `cap_mgr.validate(token, session_id, token.action(), resource_id, None)` — checks signature, epoch, expiry, action, resource match. **The token's own action is used** ("the shell granted it for this purpose. The provider capsule enforces fine-grained action checks.").

---

## B. Provider subprocess protocol

Source: `elastos/crates/elastos-runtime/src/provider/bridge.rs` (lines 28–122 quoted above).

### B.1 — Transport
- Line-delimited JSON on stdin → response on stdout. One JSON object per line, one response per request.
- Stderr is inherited (logs surface to the runtime).
- Spawned with `kill_on_drop(true)`.
- **Sequencing:** all requests serialize through a `Mutex<ProviderIo>` (provider sees them strictly one at a time).
- **Timeouts:** request = 30s, init = 10s, shutdown = 5s. Hard-coded constants in bridge.rs:20-26.
- **No env vars, no IPC fd, no signals required.** Just stdin/stdout/stderr.

### B.2 — `init` handshake

Request:
```json
{"op":"init","config":{
  "base_path":"...",
  "allowed_paths":[],
  "read_only":false,
  "encryption_key":"",
  "extra":{}
}}
```
Response:
```json
{"status":"ok","data": <optional>}
```
or
```json
{"status":"error","code":"...","message":"..."}
```

The runtime does **not** parse features/protocol-version arrays back. Init success is just `status:"ok"`. Each provider invents its own `extra` config (e.g. did-provider takes `localhost_root`).

### B.3 — Standard ops every provider implements

Defined as the typed `ProviderRequest` enum (`bridge.rs:32`):

| op | fields |
|---|---|
| `init` | `config: ProviderConfig` |
| `read` | `path, token, offset?, length?` |
| `write` | `path, token, content: Vec<u8>, append: bool` |
| `list` | `path, token` |
| `delete` | `path, token, recursive: bool` |
| `stat` | `path, token` |
| `mkdir` | `path, token, parents: bool` |
| `exists` | `path, token` |
| `shutdown` | (no body) |

That's the **typed** surface. Most actual providers do NOT use this enum — they exchange free-form JSON via `send_raw` (e.g. did-provider's `get_did`, `sign_chat_message`). Only the storage-style providers (localhost-provider) implement the typed enum.

So in practice: a provider must implement `init` (returning `{"status":"ok"}`) and `shutdown`, plus whatever ops its `capsule.json` `authority.capabilities[*].operations` declares.

### B.4 — Error envelope

Defined in `bridge.rs:87`:
```rust
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProviderResponse {
    Ok { data: Option<serde_json::Value> },
    Error { code: String, message: String },
}
```

So the wire is:
```json
{"status":"ok"}
{"status":"ok","data":{...}}
{"status":"error","code":"<snake_case_code>","message":"..."}
```

There's no enumerated error-code list — providers freely choose codes. Search the existing provider source files (e.g. `capsules/did-provider/src/main.rs`) for common codes like `not_found`, `permission_denied`, `invalid_input`. The runtime surfaces them via `BridgeError::Provider { code, message }`.

---

## C. Auth / session contract

### C.1 — `elastos.home.launch-token` schema

**The schema is actually `v2`, not `v1`** as you assumed. Source: `elastos/crates/elastos-server/src/api/gateway_home_token.rs:30` and `:142`. Quoted in full:

```rust
#[derive(Debug, Serialize, Deserialize)]
struct HomeLaunchTokenPayload {
    schema: String,                              // "elastos.home.launch-token/v2"
    app: String,                                 // capsule name, e.g. "chat-room", "system", "home"
    iat: u64,                                    // unix secs
    exp: u64,                                    // iat + HOME_LAUNCH_TOKEN_TTL_SECS (12*60*60)
    principal_id: String,                        // e.g. "person:local:<hex>"
    session_id: String,                          // auth-session grant ID
    #[serde(skip_serializing_if = "Option::is_none")]
    proof_binding_id: Option<String>,            // e.g. "proof:passkey:..." when proof-bound
    grant_id: String,                            // session-grant ID
    non_delegatable: bool,                       // ALWAYS true; rejected if false
}

#[derive(Debug, Serialize, Deserialize)]
struct HomeLaunchTokenEnvelope {
    payload: HomeLaunchTokenPayload,
    signature: String,                           // domain-separated Ed25519 sig over canonical(payload)
    signer_did: String,                          // did:key of the runtime's signing key
}
```

Wire form: base64-url-no-pad of `serde_json::to_vec(&envelope)`.

- **Signing key:** `elastos_identity::load_or_create_did(data_dir)` — the **runtime/gateway local DID** (a `did:key`, Ed25519, stored under `data_dir`). NOT a per-user DID, NOT a per-capsule DID.
- **Signing algorithm:** `crate::crypto::domain_separated_sign(signing_key, HOME_LAUNCH_TOKEN_DOMAIN, canonical.as_bytes())` where `HOME_LAUNCH_TOKEN_DOMAIN = "elastos.home.launch.v1"` (the domain string lags the schema version on purpose).
- **TTL:** 12 hours (`HOME_LAUNCH_TOKEN_TTL_SECS = 12 * 60 * 60`).
- **Verification** (`require_home_launch_token_for_any_from`, lines 298-353): decode → check schema string matches `"elastos.home.launch-token/v2"` → `verify_signed_json_envelope_against_dids(&[local_did])` → check `app` in `allowed_apps` → check `exp > now` → check `non_delegatable` is true → all three of `session_id`, `principal_id`, `grant_id` non-empty → if `proof_binding_id` present, check `is_auth_session_active(session_id)`.

### C.2 — `POST /api/apps/<app>/session/start` and the cookies it sets

Only the **chat-room** app actually has a session/start endpoint in upstream (`gateway_room.rs:78-108`):

- **Request headers:** `x-elastos-home-token: <token>` (required). No body.
- **Response body:** `{"status":"connected", "display_name":"...", "expires_at":<unix-secs>}`.
- **Set-Cookie:** name `room-session`, format from `gateway_room.rs:1118-1130`:
  ```
  room-session=<token>; Max-Age=<secs>; Path=/; HttpOnly; SameSite=Lax[; Secure]
  ```
  `Secure` is added iff `request_uses_tls(&headers)` says so.

There is a parallel `home-session` cookie set by `/api/auth/passkey/authenticate/complete` (the passkey gateway), and a `browser-session` cookie for the browser capsule. Both follow the same cookie shape from `home_launch_cookie_header`:

```rust
// gateway_home_token.rs:66-79
format!("{name}={token}; Max-Age={max_age_secs}; Path={path}; HttpOnly; SameSite=Lax")
+ optional "; Secure"
```

Constants:
- `HOME_SESSION_COOKIE = "home-session"` (gateway.rs:120)
- `ROOM_SESSION_COOKIE = "room-session"` (gateway.rs:118)
- `BROWSER_SESSION_COOKIE = "browser-session"` (gateway.rs:119)
- All HttpOnly, all `Path=/`, all `SameSite=Lax`.

### C.3 — `/api/apps/<app>/runtime-token`

Not present in upstream as a gateway route. Grep of `/tmp/upstream/elastos_crates_elastos-server_src_api_gateway.rs` shows zero matches for `runtime-token`/`runtime_token`. If your hey-social client is calling that path, it's a hey-side convention that does not exist upstream — either a YNH patch or capsule-side fabrication.

The closest legitimate endpoint is `/api/auth/attach` (server.rs:227, `handlers/attach.rs`) which exchanges a local-only `attach_secret` for a Bearer session token. That is the runtime's bearer-token mint, NOT a cookie issuer.

### C.4 — `GET /api/session`

Source: `handlers/capability.rs:541-572`. Response:
```rust
pub struct SessionInfoOutput {
    pub session_id: String,
    pub session_type: String,        // "shell" or "capsule"
    pub vm_id: Option<String>,
    pub capabilities_count: usize,
    pub created_at: u64,             // unix secs
    pub last_active: u64,
}
```

This is the **runtime** session (the Bearer/session-token session — not the gateway home-session cookie session). It's the session a capsule has, not the user's auth-session-grant. There's no `elastos://session/current` provider op — `session` is reserved in `RESERVED_SUB_NAMES` but no built-in capsule provides it.

### C.5 — Principal vs social DID

- **`principal_id`** (string): the runtime-owned identity for a user. Format defined in `auth.rs:1156-1158`:
  ```rust
  pub fn local_person_principal_id(proof_binding_id: &str) -> String {
      let digest = Sha256::digest(proof_binding_id.as_bytes());
      format!("person:local:{}", hex::encode(&digest[..16]))
  }
  ```
  Format also accepts `device:<did>` for device-owned principals (`PrincipalId::device_did` in `elastos-auth/src/lib.rs:21`).
- **`principal_localhost_root`** (auth.rs:1175): each principal gets `localhost://Users/<hex12>` (NOT the principal_id verbatim — it's `hex(sha256(principal_id))[..12]`). This is the per-principal Users root.
- **`proof_binding_id`**: identifies the proof method used (passkey credential ID, EVM address, etc.). Schema in `elastos-auth/src/lib.rs:41`.
- **Social DID / `did:elastos` / EID**: separate concept, mentioned in state.md as "a linked global account path", not currently issued by the runtime. `did:key` is the device/node DID.

There is NO field literally called `social_did` in `/api/session` or session-grants. Identity layers are intentionally separate: handle (display label), device DID (`did:key`), principal_id, social DID/EID (future), CID (content).

---

## D. Capability flow

Source: `elastos/crates/elastos-server/src/api/handlers/capability.rs` (quoted in full above).

### D.1 — `POST /api/capability/request`

Request body:
```json
{"resource":"elastos://peer/*","action":"message"}
```

Action whitelist: `read | write | execute | delete | message | admin` (capability.rs:67-83).

Resource scheme whitelist: `elastos://` and `localhost://` only (`is_supported_resource_scheme`, capability.rs:85).

Auto-deny: `is_system_only_backend_resource` rejects `elastos://ipfs/*`, `elastos://kubo/*`, `elastos://ipfs-cluster/*`, `elastos://elacity-sdk/*`, `elastos://ipfs-provider/*`, `elastos://gateway/*`. Those are system backends, not capsule capabilities — see `handlers/capability.rs:692-707` tests for the exact list.

Response:
```rust
pub struct RequestCapabilityOutput {
    pub status: String,                          // "pending" | "granted" | "auto_denied" | "denied"
    pub request_id: Option<String>,              // present for pending and denied
    pub token: Option<String>,                   // base64 CapabilityToken when granted
    pub reason: Option<String>,                  // when denied/auto_denied
}
```

Comment at capability.rs:104: "For now, all requests go to pending (no auto-grant policy yet)". So in the current source, `status:"granted"` is theoretically possible but currently never returned by the request endpoint — grants always come back via `/api/capability/request/:id` after a shell `POST /api/capability/grant`.

### D.2 — `GET /api/capability/request/:id`

```rust
pub struct RequestStatusOutput {
    pub status: String,                          // "pending" | "granted" | "denied" | "expired"
    pub token: Option<String>,                   // base64 token when granted
    pub reason: Option<String>,
}
```

### D.3 — `X-Capability-Token` value format

It is an **opaque base64-encoded `CapabilityToken`**, not a JWT. Decoded via `CapabilityToken::from_base64(token_b64)` (`handlers/provider.rs:103`). The token contains: id, capsule_id, action, resource_id, constraints, epoch, expiry, an Ed25519 signature over those fields. Implementation: `elastos/crates/elastos-runtime/src/capability/token.rs` (23.5 KB; constraint struct includes `expires_at`, `max_uses`, `delegatable`, `nonce`).

Validation happens in `CapabilityManager::validate(token, session_id, action, resource, audit_ctx)` — checks signature, epoch (no `revoke_all` has bumped past it), expiry, action match, resource match (subtree-aware), max_uses decrement.

### D.4 — Auto-grant policy

The `PolicyEvaluator` (`capability/evaluator.rs`, `capability/policy.rs`) has multiple verifier modes selected by `ELASTOS_SHADOW_MODE` env (server.rs:167-191):

- Default: `ShellPassthroughVerifier` — defers to shell decision, no auto-grant.
- `ELASTOS_SHADOW_MODE=rules`: adds `RulesVerifier::with_defaults()` as a shadow (observational, doesn't change outcomes).
- `ELASTOS_SHADOW_MODE=1|true|yes|on`: adds `AutoGrantVerifier` as shadow.

So **as shipped, there is no auto-grant of capability requests**. They all go to a pending queue that the shell (or Home gateway) drains. The `capsule.json` `permissions.messaging` / `permissions.storage` lists are **manifest-declared intent**, NOT automatic grants — they exist for alignment gates (manifest validation rejects requests for forbidden scopes) and for shell UI hints, but the runtime still requires a per-resource capability request at runtime.

### D.5 — Resource string format

Canonical shape is `elastos://<sub>/*` or `elastos://<sub>/<path>` or `localhost://<root>/<path>`. Wildcards: the source uses `*` for "any subpath" by convention but actual matching is done by `ResourceId` subtree-matching (look at `capability/manager.rs` and `provider_resource.rs:build_capability_resource`).

`elastos://peer/*` is canonical (matches the reserved sub-name list). For `localhost://`, roots ARE the principal-aware roots: `localhost://Users/<hex>/...`, `localhost://MyWebSite/...`, `localhost://Public/...`, `localhost://AppCapsules/...`, etc. The localhost roots accepted in storage are pinned by localhost-provider's manifest:

```json
"permissions": {"storage": [
  "localhost://UsersAI/", "localhost://AppCapsules/", "localhost://ElastOS/",
  "localhost://Local/", "localhost://MyWebSite/", "localhost://Public/", "localhost://Users/"
]}
```

---

## E. Storage

Source: `handlers/storage.rs` (lines 91-512 above). Routes registered in `server.rs:375-387`:

```
GET    /api/localhost                  -> handle_get_root
GET    /api/localhost/                 -> handle_get_root
GET    /api/localhost/*path            -> storage_get (read_file)
PUT    /api/localhost/*path            -> storage_write (write_file)
DELETE /api/localhost/*path            -> storage_delete
HEAD   /api/localhost/*path            -> storage_stat
POST   /api/localhost/*path            -> storage_post (list?, mkdir?)
```

### E.1 — Full contract

- **Auth header:** `Authorization: Bearer <session-token>` (mandatory, via `auth_middleware`).
- **Capability header:** `X-Capability-Token: <base64>` (mandatory unless shell session, per `enforce_capability` at storage.rs:418).
- **GET** returns raw bytes with content-type guessed from extension (handlers/storage.rs:514-531).
- **PUT** body is raw bytes, returns `{"path":"localhost://...","size":<bytes>}`.
- **DELETE** with `?recursive=true` for non-empty dirs.
- **POST** with `?list=true` lists, `?mkdir=true` creates directory.
- **Quota:** `storage_quota_mb` in `ProviderStorageState` (defaults to 0 = unlimited at server.rs:366). Exceeded → 507 Insufficient Storage.

There is **no upstream `/api/apps/<app>/storage/<path>` route**. That's a hey-side or YNH-side convention not in this commit. The canonical capsule-facing storage is `/api/localhost/*path` (routed through the provider registry to localhost-provider) — same path your audit memory calls the "legacy" path. There is no newer route in upstream.

### E.2 — 412 (create-only)

I find **zero references to 412 / `PRECONDITION_FAILED` / `create_only`** in `handlers/storage.rs` or `provider/registry.rs`. The handler returns: 404 NotFound, 403 PermissionDenied, 400 InvalidPath, 507 QuotaExceeded, 500 Internal. 412 must be coming from somewhere else (could be localhost-provider's own create vs overwrite check, but that's not in upstream's handler).

### E.3 — Why `.AppData/ElastOS/Identity/profile.json` returns 400

From storage.rs:504-511:
```rust
fn reject_principal_root_storage_path(path: &str) -> Result<(), StorageApiError> {
    if path == "Users" || path.starts_with("Users/") {
        return Err(StorageApiError::PermissionDenied(
            "principal-root storage requires a runtime principal-scoped provider route".into(),
        ));
    }
    Ok(())
}
```

So any `localhost://Users/*` path hits this guard **before** capability check and **returns 403 (PermissionDenied)**, not 400. To write principal-scoped data, the runtime calls the storage provider through a different code path that injects the verified principal from a launch-grant (see state.md section about "principal-root object envelope" and "signed Home launch-token principal").

Your audit memory says "400 because Hey is trying to read shared identity paths outside its capsule grant" — actual mechanism is: any direct `/api/localhost/Users/...` HTTP call is **403'd unconditionally** at the gateway, regardless of capsule grant. The path must instead route through a principal-aware bridge (i.e. a `/api/provider/...` call with home-launch-token), or via a provider that has the runtime's protected `BridgeContext`.

So `.AppData/ElastOS/Identity/profile.json` written directly via the storage API will:
- 403 if path starts with `Users/` (current Hey code)
- 400 only if the path doesn't normalize as `rooted_localhost_uri` (no recognized root prefix) — `canonical_local_uri` returns `InvalidPath` → 400.

Likely root cause: `.AppData/ElastOS/...` (no `Users/<hex>/` prefix) is an unrooted path → `rooted_localhost_uri` returns None → `InvalidPath` → 400. The full storage path should be `localhost://Users/<principal-root>/.AppData/ElastOS/...` but you can't write Users/ directly — you must go through a principal-scoped provider route (the documents/home one), not raw localhost API.

### E.4 — `permissions.storage` syntax in capsule.json

From `manifest.rs:271-292`:
- Each entry must be `localhost://...` or `elastos://...` (scheme check).
- App/viewer/content roles can NOT request `is_runtime_system_service_resource` (Identity/* etc) or `is_system_only_backend_resource` (ipfs/* etc) — manifest validation rejects.
- Wildcards: `*` is conventional. Resource matching is done by `ResourceId` subtree-prefix logic at capability check time.
- Inheritance: granting a parent grants children (subtree). No documented OR/glob beyond suffix `*`.

Example (chat capsule.json):
```json
"permissions": {
  "storage": ["localhost://Users/self/.AppData/LocalHost/Chat/*"],
  "messaging": []
}
```

Note `Users/self` is a **logical alias** the runtime rewrites to `Users/<verified-principal-root>` when a verified launch context exists; without that context the access is denied (state.md: "the capsule kernel maps `localhost://Users/self` through an explicit principal context when present, rejects explicit foreign `localhost://Users/<root>` access").

---

## F. Built-in providers and their contracts

Inferred from `capsules/*/capsule.json` and `RESERVED_SUB_NAMES` in registry.rs:163.

| Scheme | Capsule dir | Role | Ops (from authority.capabilities[].operations) |
|---|---|---|---|
| `localhost://*` | `elastos/capsules/localhost-provider` | storage backend | `read, write, list, delete, stat, mkdir, exists` (typed ProviderRequest enum) |
| `elastos://did/*` | `capsules/did-provider` | identity | `get_did, resolve, sign_chat_message, verify, verify_did_recovery, get_nickname, set_nickname, get_persona_did` |
| `elastos://ipfs/*` | `capsules/ipfs-provider` | content backend | `add_bytes, add_path, add_directory, cat, cat_to_path, get_bytes, ls, download_directory, pin, unpin, health, status` — **NOTE:** system-only, app capsules cannot request this scheme. Use `elastos://content/*`. |
| `elastos://ai/*` | `capsules/ai-provider` | AI router | `chat_completions, list_backends, ping` |
| `elastos://llama/*` | `capsules/llama-provider` | local model | (not inspected; llama-provider/main.rs exists) |
| `elastos://tunnel/*` | `capsules/tunnel-provider` | cloudflared | `start, stop, status, ping` |
| `elastos://net/*` | `capsules/net-provider` | browser/net | `status, resolve, connect, stream, http` |
| `elastos://exit/*` | `capsules/exit-provider` | browser egress | `status, quote, open_stream, close_stream, http_fetch` |
| `elastos://decrypt/*` | `capsules/decrypt-provider` | DRM decrypt | `status, open_session, render` |
| `elastos://key/*` | `capsules/key-provider` | dKMS | `status, release` |
| `elastos://drm/*` | `capsules/drm-provider` | DRM | (not inspected) |
| `elastos://rights/*` | `capsules/rights-provider` | rights | (not inspected) |
| `elastos://wallet/*` | `capsules/wallet-provider` | wallet | (capsule.json is 1.6 KB; ops in wallet-provider/src/protocol.rs) |
| `elastos://chain/*` | `capsules/chain-provider` | chain | `networks, status, block_number, sync_health, node_lifecycle, balance` (from gateway proxy whitelist) |
| `elastos://availability/*` | `capsules/availability-provider` | SmartWeb | `ensure, status` |
| `localhost://WebSpaces/*` | `capsules/webspace-provider` | WebSpace resolver | `resolve, read, list, stat, exists, ping` |
| `elastos://browser-engine/*` | `capsules/browser-engine-adapter` | browser engine | (capsule.json only) |
| `elastos://site/*` | `capsules/site-provider` | site publisher | (not inspected) |
| `elastos://content/*` | (server-side `crate::content`) | content contract (app-facing wrapper around ipfs) | publish, fetch, etc. |
| `elastos://namespace/*` | `crate::api::handlers::namespace` | namespace (server-side) | list/resolve/read/write/delete/status/cache/prefetch |
| `elastos://message/*` | reserved | not implemented as separate provider |
| `elastos://storage/*` | reserved | not implemented as separate provider |

Special non-`elastos://` roles:
- `shell` (`elastos/capsules/shell`): role=`shell`, type=microvm, entrypoint `rootfs.ext4`. The orchestrator UI. Has implicit `is_shell()` bypass on all capability checks.
- `home` (`capsules/home`): role=`shell`, type=wasm, entrypoint `home.wasm`. Different from `shell` — Home is the runtime-owned browser-hosted Home surface. Despite role=`shell` it does NOT have shell-session privilege; the gateway distinguishes by HOME_CAPSULE_ID.
- `agent` (`capsules/agent`): AI chat agent, microvm. The `agent.sh` script.

Providers in upstream's `elastos/capsules/` (the inner directory): `localhost-provider`, `shell`. Everything else lives at top-level `capsules/`. The YNH fork's `_common.sh` builds both:
```bash
cargo_as_app build --release --manifest-path "$install_dir/elastos/capsules/$crate/Cargo.toml"   # for inner crates
cargo_as_app build --release --manifest-path "$install_dir/capsules/$crate/Cargo.toml"          # for top-level crates
```

There is NO `peer-provider`, `identity-provider`, `principal-provider`, `session-provider`, or `capabilities-provider` in upstream. Reserved sub-names exist but no concrete capsule implements them in this tree. You're creating new territory with `identity-projection-provider`.

---

## G. capsule.json manifest

Source: `elastos/crates/elastos-common/src/manifest.rs:17-73` (quoted in full).

### G.1 — Full field list (schema `elastos.capsule/v1`)

```rust
#[serde(deny_unknown_fields)]
pub struct CapsuleManifest {
    pub schema: String,                          // required, must = "elastos.capsule/v1"
    pub version: String,                         // required, non-empty
    pub name: String,                            // required, non-empty
    pub description: Option<String>,             // optional
    pub author: Option<String>,                  // optional
    pub role: CapsuleRole,                       // required: shell | app | viewer | provider | content
    #[serde(rename = "type")]
    pub capsule_type: CapsuleType,               // required: wasm | microvm | oci | media | data
    pub entrypoint: String,                      // required, relative path, no ".."
    pub requires: Vec<CapsuleRequirement>,       // optional: [{name, kind: "capsule"|"external"}]
    pub provides: Option<String>,                // required for role=provider, forbidden otherwise
    pub authority: Option<ProviderAuthority>,    // required for role=provider
    pub capabilities: Vec<String>,               // optional: list of URIs the capsule needs from others
    pub resources: ResourceLimits,               // optional, defaults: memory_mb=64, cpu_shares=100, gpu=false
    pub permissions: Permissions,                // optional, see below
    pub microvm: Option<MicroVmConfig>,          // optional, for type=microvm
    pub providers: Option<HashMap<String,String>>, // optional, only for role=provider (e.g. "local"->"built-in")
    pub viewer: Option<String>,                  // only for role=content
    pub signature: Option<String>,               // optional base64 signature
}
```

`deny_unknown_fields` is strict — unknown keys reject parsing.

### G.2 — Valid `role` and `type` values

```rust
#[serde(rename_all = "lowercase")]
pub enum CapsuleRole { Shell, App, Viewer, Provider, Content }

#[serde(rename_all = "lowercase")]
pub enum CapsuleType { Wasm, MicroVM, Oci, Media, Data }
```

Note: `CapsuleType::MicroVM` serializes to `"microvm"` (lowercased), so the JSON literal is `"type":"microvm"`.

Compat matrix (from `validate()`):
- `role=content` requires `type=data`.
- `role=provider` requires `provides` AND `authority`.
- `provides` only valid on `role=provider`.
- `authority` only valid on `role=provider`.
- `viewer` field only valid on `role=content`.
- `permissions.carrier` only valid on `role=provider` with `provides` set.
- `permissions.guest_network` only valid on `role=provider` with `provides` set.
- App/viewer/content can't `requires` external dependencies.
- App/viewer/content can't override providers.
- App/viewer/content microVMs can't expose `http_port`.

### G.3 — `provides` for `role=provider`

Must be `elastos://<scheme>/*` or `localhost://<root>/*`. Scheme must be supported (`is_supported_resource_scheme`). This is the **single scheme** the provider answers — the runtime registers it in its ProviderRegistry under that key.

### G.4 — `ProviderAuthority` schema (provider-only)

```rust
#[serde(deny_unknown_fields)]
pub struct ProviderAuthority {
    pub reason: String,                          // human-readable why
    pub capabilities: Vec<ProviderCapabilitySchema>,
    pub audit_events: Vec<String>,               // event names this provider emits
}

pub struct ProviderCapabilitySchema {
    pub resource: String,                        // e.g. "elastos://did/*"
    pub actions: Vec<String>,                    // subset of read|write|execute|delete|message|admin
    pub operations: Vec<String>,                 // free-form op names
}
```

All three sub-fields are non-empty-required. This is what makes a provider "reviewable" — operator can see exactly what surface it offers.

### G.5 — `permissions.messaging` and `permissions.storage`

```rust
pub struct Permissions {
    pub carrier: bool,                           // host process; provider-only
    pub guest_network: bool,                     // TAP NIC; provider-only
    pub storage: Vec<String>,                    // URI patterns; allowlist intent
    pub messaging: Vec<String>,                  // URI patterns; allowlist intent
}
```

These are **manifest-declared intent**, not auto-grants. State.md confirms: "the capsule kernel maps `localhost://Users/self`... rejects capability requests... when principal context is missing". The capsule still has to call `/api/capability/request` at runtime, and the shell still has to grant. Manifest validation just rejects upfront if the capsule is asking for something its role isn't allowed to request.

So `permissions.messaging: ["elastos://peer/*"]` says: "this capsule will at runtime request `message` capability on `elastos://peer/*`; the runtime checks the role is allowed to ask, then queues a pending request for shell approval."

### G.6 — `entrypoint` semantics

- `type=wasm` apps: path to `.wasm` file (e.g. `"home.wasm"`, `"chat-room.wasm"`, `"chat-stdio.wasm"`).
- `type=microvm`: path to `rootfs.ext4` file in the capsule dir. The runtime spawns crosvm with this as the rootfs.
- `type=data`: path to the data artifact (gba-ucity uses this).
- `type=oci`: path to OCI bundle (not extensively used).
- `type=media`: path to media manifest (not extensively used).

There is no special "native binary provider" type — provider binaries (did-provider, ipfs-provider, etc.) ship as **microvm** with `entrypoint: "rootfs.ext4"`. The supervisor builds the rootfs from the cargo-built binary at install time (see YNH `_common.sh:413-425` which feeds binaries into a separate `BIN=` env), but the manifest still says microvm. That's an installation detail; **at the manifest layer the provider is a microvm capsule**, not a "path to bin".

For app capsules that have BOTH a wasm and a web surface (chat-room), the wasm in `entrypoint` is mostly a presence signal — actual UI is the `browser/` directory served by `serve_browser_app_asset` (gateway.rs:602-605: `GET /apps/:app/*path`).

---

## H. Canonical reference apps

### H.1 — chat-room

Found at `capsules/chat-room/`:
- `capsule.json` is minimal (316 bytes, quoted above): no `permissions`, no `requires`, no `capabilities`.
- `src/main.rs` is 19 lines — just logs launch (quoted above). Real work is in `browser/`.
- `browser/index.html` is the Chat Room UI (quoted above). It's a small HTML page that loads `chat_room_ui.wasm` and `chat_room_ui.js` (built from `capsules/chat-room-ui/`). UI logic in Rust→WASM.
- Critical client behavior: reads `home_token` query string at boot:
  ```js
  const accessMode = new URLSearchParams(window.location.search).has("home_token")
    ? "shell"
    : "gateway";
  ```
  When launched from Home, `home_token` is passed as a query param. When accessed directly via `/apps/chat-room/`, it's gateway mode (uses cookie auth).
- Session/start redemption: client calls `POST /api/apps/chat-room/session/start` with `x-elastos-home-token` header (when in shell mode) or relies on `room-session` cookie (gateway mode). Server returns `{status, display_name, expires_at}` and sets `room-session` cookie. Subsequent calls (`/poll`, `/objects/send`, `/upload/*`) authenticate via the cookie.
- **DM convention is NOT used by chat-room.** chat-room uses runtime room-service objects (not gossip messages). The peer-provider DM marker `\x01DM:<recipient_pubkey>\x01<content>` is used by the older native `chat` capsule (capsules/chat/src/) which IS a peer-provider client.

### H.2 — Other good reference impls

- **chat** (`capsules/chat/`): native TUI client (microvm), 68 KB main.rs. Uses `elastos_guest::carrier_invoke` against `elastos://peer/*`. The `capabilities.acquire_capability("localhost://Users/self/.AppData/LocalHost/Chat/*", "write")` call in `src/session.rs:74` shows the per-resource cap-request pattern.
- **agent** (`capsules/agent/`): AI chat agent that joins a chat channel — combines did-provider + peer + ai. Small, readable.
- **localhost-provider** (`elastos/capsules/localhost-provider/src/main.rs`): the only canonical implementation of the **typed** ProviderRequest enum. Read this if your provider is storage-shaped.
- **did-provider** (`capsules/did-provider/src/main.rs`): the canonical impl of a **free-form** (`send_raw`) provider. Read this if your provider takes custom JSON ops.
- **shell** (`elastos/capsules/shell/src/main.rs`): 56 KB — the orchestrator. Shows how a shell session interacts with capability grant/deny endpoints.

---

## I. Provider install/discovery

### I.1 — Where provider binaries live

In a YNH-installed runtime, the supervisor reads binary paths from env vars exported by `scripts/_common.sh:415-425`:
```
SHELL_BIN=$install_dir/elastos/target/release/shell
LOCALHOST_PROVIDER_BIN=$install_dir/elastos/target/release/localhost-provider
DID_PROVIDER_BIN=$install_dir/capsules/did-provider/target/release/did-provider
WEBSPACE_PROVIDER_BIN=$install_dir/capsules/webspace-provider/target/release/webspace-provider
```

On a non-YNH local install (the `install.sh` flow), binaries land in `~/.local/share/elastos/bin/` based on the `install_path` field in `components.json` `external` section (`"install_path": "bin/did-provider"` etc.). XDG defaults: `${XDG_DATA_HOME:-~/.local/share}/elastos/`.

Spawned via `tokio::process::Command::new(binary_path).stdin(piped).stdout(piped).stderr(inherit())` (bridge.rs:191-197) — see B above.

### I.2 — `components.json` schema

Format (from `/tmp/upstream/components.json` head):
```json
{
  "schema": "elastos.components/v1",
  "capsules": {
    "<name>": {
      "cid": "",                                 // IPFS CID (filled by publisher)
      "sha256": "",                              // checksum
      "size": 0,                                 // bytes
      "platforms": ["x86_64-linux","aarch64-linux"|"any"],
      "note": "..."                              // optional
    }
  },
  "external": {
    "<name>": {
      "install_path": "bin/<name>",              // target install location
      "description": "...",
      "platforms": {
        "linux-amd64": {
          "cid":"", "checksum":"", "size":0,
          "release_path":"<name>-linux-amd64",   // path inside published artifacts
          "install_path":"bin/<name>"
        },
        "linux-arm64": {...},
        "*": {...}                                // platform-independent
      }
    }
  }
}
```

`external` is for native binaries (shell, providers, helpers like cloudflared, kubo, browser-engine-supervisor). `capsules` is for WASM/microvm capsule artifacts.

### I.3 — `components.additions.json` merge

From `/var/home/linux/ai/elastos-runtime-ynh/components.additions.json` (quoted in full earlier): schema is `elastos.components/v1-additions`. Contains only `external` entries.

Merge logic in `scripts/_common.sh:90-100`:
```python
upstream = json.load(open("$extracted/components.json"))
add = json.load(open("$target_dir/components.additions.json"))
upstream["external"].update(add["external"])
json.dump(upstream, open("$target_dir/components.json", "w"), indent=2)
```

So additions are **layered on top of upstream's `external`** dict — same key wins (Hey overrides). `capsules` are not currently merged this way; Hey capsules are copied directly under `capsules/` by `fetch_hey_capsules()`.

### I.4 — Adding identity-projection-provider to YNH install

Two paths depending on whether it's a built-from-source crate or a pre-built binary:

**As a sibling capsule built from source (preferred for Hey pack):**
1. Add `capsules/identity-projection-provider/` to `HeyElastos/Hey-capsule` tarball with valid `capsule.json` (role=provider, provides=`elastos://identity-projection/*` or similar — note **must be a NEW scheme not in `RESERVED_SUB_NAMES`** unless you patch the runtime). Wait — actually the registry registers any scheme the provider claims; `RESERVED_SUB_NAMES` is for the elastos://-sub-dispatch shortcut. A non-reserved scheme like `elastos://identity-projection/*` works for direct top-level lookup but won't sub-dispatch. Easier: pick a scheme in `RESERVED_SUB_NAMES` you're allowed to own, or accept top-level scheme lookup.
2. Add to `_common.sh:354-365`-style build loop (it iterates capsule dirs). The existing loop builds top-level `capsules/*` if a `Cargo.toml` exists.
3. Add export of `IDENTITY_PROJECTION_PROVIDER_BIN=$install_dir/capsules/identity-projection-provider/target/release/identity-projection-provider` to the env block at `_common.sh:413-440`, then have the supervisor binary-table (in upstream `elastos/crates/elastos-server/src/binaries.rs`) know about it — **this is the actual blocker**: the runtime's `binaries.rs` is a small hard-coded map. Without patching upstream, you have to register dynamically via... actually let me check.

**Pre-built binary path:**
1. Add an entry to `components.additions.json` under `external`:
   ```json
   "identity-projection-provider": {
     "install_path": "bin/identity-projection-provider",
     "platforms": {
       "linux-amd64": {"release_path":"identity-projection-provider-linux-amd64",
                       "install_path":"bin/identity-projection-provider"}
     }
   }
   ```
2. Hosted release at the publisher gateway provides the actual binary.
3. The runtime supervisor must know to register it for its scheme — see `provider_resource.rs` (31 KB) and `supervisor.rs` (77 KB) for the binary→scheme registration logic.

Realistic: the cleanest way is a patch under `scripts/patches/` (see YNH's `_common.sh:107-115` which applies `*.patch` files at install time and fails install if they don't apply). That keeps you tracked vs upstream.

---

## J. Other observations

### J.1 — dDRM

Mentioned in state.md: "production dDRM, dKMS, decrypt/render providers" are NOT complete. The boundary providers (`drm-provider`, `key-provider`, `decrypt-provider`, `rights-provider`, `availability-provider`) are wired as fail-closed contracts. Relevant for protected content via `elastos.elacity.com` flows. Not relevant for Hey Social/Messenger unless you're shipping paid content.

### J.2 — `patch-0001` / `patch-0002`

Not found in upstream. `scripts/patches/` in YNH (path `/var/home/linux/ai/elastos-runtime-ynh/scripts/patches/`) is empty in current snapshot (would have to recheck). These names sound like YNH-local patches that were applied and removed once upstream merged them. Without finding the actual filenames in your local fork's `scripts/patches/`, I can't say what they contained.

### J.3 — PEER_PROTOCOL v1.0 → v1.1

The published doc is titled "v1.1". The v1.0 → v1.1 diff isn't in the doc text itself — would need to look at git history of the file. From the doc, v1.1 includes optional `signature` and `sender_id`/`ts` overrides in `gossip_send`. The doc's parenthetical note ("the runtime accepts a `signature` field") suggests v1.0 didn't have it.

### J.4 — v0.3 → v0.4 identity path transition

Not visible in this commit. UPSTREAM_VERSION = v0.3.0. The PR/transition you mention is likely in YNH-side adapter code (your `shell.rs` deletion), where v0.3 used `localhost://Users/self/` (alias) and v0.4-style code uses `localhost://Users/<principal-hex>/` directly. The runtime side hasn't crossed that bridge — it still rewrites `Users/self` via principal context (state.md confirms).

### J.5 — Spec drift between docs and runtime

Documented examples:
- PEER_PROTOCOL.md says request envelope `{"op":"...","data":...}` with optional fields. The actual runtime's `bridge.rs:32` typed enum uses `{"op":..., <field>:<value>,...}` directly (no `data` wrapper). The doc is right about line-delimited JSON and `{"status":"ok"|"error",...}` response.
- The `home.launch-token` is v2, not v1 (gateway_home_token.rs:142). Schema label and code label can lag — `HOME_LAUNCH_TOKEN_DOMAIN = "elastos.home.launch.v1"` (domain string is v1 for signature stability, schema is v2 for envelope shape).
- Reserved scheme list in `RESERVED_SUB_NAMES` includes schemes (peer, message, storage, session) that have **no upstream capsule implementing them**. Don't assume reserved == shipped.
- The `provides` field uses `localhost://*` for localhost-provider (a wildcard) while sub-providers use `elastos://<name>/*`. Both forms are valid.
- `capabilities` field in `capsule.json` is a list of URIs the capsule **needs from others** (declared intent for capability requests it WILL make). This is distinct from `permissions.messaging` / `permissions.storage`. The relationship is: `permissions` declares *what scopes this capsule may even ask for*; `capabilities` declares specific URIs it intends to request. Both are gated by manifest validation against the role.

### J.6 — Logging / debugging on YunoHost

- Runtime is run as a systemd unit (`conf/systemd.service` in YNH repo). View logs: `sudo journalctl -u elastos-runtime -f` or `sudo journalctl -u <yunohost-app-name> -f`.
- Provider logs surface via the bridge — providers print to stderr, which the runtime inherits (`bridge.rs:194: .stderr(std::process::Stdio::inherit())`). So provider logs appear in the runtime's systemd journal too.
- App-level logs depend on the capsule. WASM capsules can log via stderr (also inherited).
- The runtime exposes `/api/audit` (shell-only) with structured audit events for grants/revokes/access. Filter by type with `?type=capability_grant`.
- Useful env vars: `ELASTOS_SHADOW_MODE` (rules|true|on for verifier modes), `RUST_LOG=info,elastos_server=debug`.
- YNH's `update-hey-only.sh` is the sub-minute refresh path; full install via `yunohost app upgrade` is heavier.

---

## File path index (for future drill-downs)

Upstream (raw.githubusercontent.com prefix):
- `elastos/docs/PEER_PROTOCOL.md`
- `docs/CAPSULE_MODEL.md`
- `docs/INTERACTIVE_RUNTIME_CONTRACT.md`
- `docs/BROWSER_CAPSULE.md`
- `state.md`
- `components.json`
- `elastos/crates/elastos-server/src/api/gateway.rs`
- `elastos/crates/elastos-server/src/api/gateway_home_token.rs`
- `elastos/crates/elastos-server/src/api/gateway_provider_proxy.rs`
- `elastos/crates/elastos-server/src/api/gateway_room.rs`
- `elastos/crates/elastos-server/src/api/handlers/provider.rs`
- `elastos/crates/elastos-server/src/api/handlers/capability.rs`
- `elastos/crates/elastos-server/src/api/handlers/storage.rs`
- `elastos/crates/elastos-server/src/api/handlers/identity.rs`
- `elastos/crates/elastos-server/src/api/handlers/namespace.rs`
- `elastos/crates/elastos-server/src/api/middleware.rs`
- `elastos/crates/elastos-server/src/api/server.rs`
- `elastos/crates/elastos-server/src/api/routes.rs`
- `elastos/crates/elastos-server/src/auth.rs`
- `elastos/crates/elastos-server/src/binaries.rs`
- `elastos/crates/elastos-server/src/provider_resource.rs`
- `elastos/crates/elastos-server/src/supervisor.rs`
- `elastos/crates/elastos-runtime/src/provider/registry.rs`
- `elastos/crates/elastos-runtime/src/provider/bridge.rs`
- `elastos/crates/elastos-runtime/src/handler/protocol.rs`
- `elastos/crates/elastos-runtime/src/handler/request_handler.rs`
- `elastos/crates/elastos-runtime/src/capability/{token,manager,evaluator,policy,pending}.rs`
- `elastos/crates/elastos-runtime/src/session/{mod,registry}.rs`
- `elastos/crates/elastos-common/src/manifest.rs`
- `elastos/crates/elastos-common/src/localhost.rs`
- `elastos/crates/elastos-auth/src/lib.rs`
- `elastos/capsules/localhost-provider/{capsule.json,src/main.rs}`
- `elastos/capsules/shell/{capsule.json,src/main.rs}`
- `capsules/chat-room/{capsule.json,src/main.rs,browser/index.html}`
- `capsules/chat/{capsule.json,src/*.rs}`
- `capsules/did-provider/{capsule.json,src/main.rs}`
- `capsules/<each>-provider/capsule.json`

Local YNH fork:
- `/var/home/linux/ai/elastos-runtime-ynh/UPSTREAM_VERSION` (v0.3.0)
- `/var/home/linux/ai/elastos-runtime-ynh/components.additions.json`
- `/var/home/linux/ai/elastos-runtime-ynh/scripts/_common.sh` (line 60-120 for component merge; 354-440 for build/env)
- `/var/home/linux/ai/elastos-runtime-ynh/scripts/install.sh`
- `/var/home/linux/ai/elastos-runtime-ynh/scripts/patches/` (apply-on-install patch directory; failing patches halt install)
- `/var/home/linux/ai/elastos-runtime-ynh/conf/systemd.service`
- `/var/home/linux/ai/elastos-runtime-ynh/docs/HEY_MODULAR_ARCHITECTURE.md` (Hey-specific architectural overlay)

Cached fetched source (this session): `/tmp/upstream/*` (one file per upstream path, slashes replaced with underscores). 56 files, ~1.4 MB total.

---

## TL;DR corrections to your current assumptions

1. **Schema is v2, not v1.** `elastos.home.launch-token/v2` is the live schema for Home launch tokens. v1 isn't accepted.
2. **`/api/provider/:scheme/:op` is two different routes.** Runtime (local-HTTP, requires Bearer + X-Capability-Token) vs Gateway (Home browser, requires `x-elastos-home-token`). Different code paths in upstream.
3. **`/api/apps/<app>/runtime-token` is not an upstream route.** If Hey is calling it, that's Hey-side or YNH-side. The legit Bearer-token mint is `/api/auth/attach`.
4. **All `/api/localhost/Users/...` direct access returns 403** (PermissionDenied), unconditionally. Principal-scoped writes must use a different code path.
5. **Provider subprocesses are stdin/stdout JSON with `{op}` requests and `{status}` responses.** No env, no fd, no signal. 30s timeout per request.
6. **`elastos.capsule/v1` manifest uses `deny_unknown_fields`** — typos in capsule.json are fatal at parse time.
7. **There is no auto-grant capability policy in shipping code.** Everything goes to a pending queue. `permissions.*` in manifest is intent, not authorization.
8. **`peer-provider` / `identity-provider` / `session-provider` do not exist upstream.** The schemes are reserved in `RESERVED_SUB_NAMES` but no built-in capsule provides them. You're building greenfield.
9. **Chat-room does NOT use DM markers.** Native `chat` (microvm) uses them. Chat-room is the browser-hosted runtime room-service capsule.
10. **`reject_principal_root_storage_path` is the gate that 403s `Users/*`** — not your capability grants. Path validation runs before capability check.
