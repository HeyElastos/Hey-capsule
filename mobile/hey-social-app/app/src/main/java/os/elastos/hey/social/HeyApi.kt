package os.elastos.hey.social

import org.json.JSONArray
import org.json.JSONObject
import kotlinx.coroutines.launch

/**
 * 1:1 Kotlin wrapper over the native app-API (libhey_mobile_runtime.so,
 * `Java_os_elastos_hey_social_HeyApi_*`). All logic + crypto run in Rust/hey-core;
 * Kotlin only marshals JSON / bytes. Call from a background coroutine.
 */
object HeyApi {
    const val PORT = 8787
    /** Sentinel passed as hey_init's identityBlob to request a HEADLESS-VAULT cold
     *  start (vault ON, seed sealed). The runtime treats it as "never generate" — it
     *  boots from carrier-identity.json or fails closed, but NEVER mints a fresh
     *  identity (which would fork the vaulted account). MUST byte-match the Rust
     *  HEADLESS_BOOT_SENTINEL in capsules/hey-mobile-runtime/src/lib.rs. */
    private const val HEADLESS_BOOT = "__hey_headless_boot__"

    @Volatile private var started = false
    /** Read-only: is the in-process runtime up? Used to gate idempotent resume-time
     *  re-arming of the storage DEK (don't poke the JNI before hey_init). */
    val isStarted: Boolean get() = started
    /** Unsealed identity seed (set after a vault unlock). The runtime starts from
     *  this when the vault is on, so the seed is never read from a plaintext file. */
    @Volatile var unlockedSeed: String? = null

    init {
        System.loadLibrary("hey_mobile_runtime")
    }

    /**
     * Start the in-process runtime exactly once per process (Activity + Service).
     * When the vault is ON and sealed but we have no unsealed seed yet, REFUSE to
     * start — otherwise the runtime would mint a fresh plaintext identity behind
     * the user's back. Returns true if the runtime is (now) running.
     */
    @Synchronized
    fun ensureStarted(ctx: android.content.Context, seed: String? = null): Boolean {
        if (started) return true
        // V6: idempotent cold-start reconciler — self-heal the enableVault seal→delete
        // crash window. enableVault seals → verifies → persists the carrier blob →
        // setOn(true) → deletes identity.json LAST. If the process died AFTER setOn but
        // BEFORE deleteIdentity, the vault is ON + sealed yet a stale identity.json still
        // exists; on next cold start that leftover would shadow the headless boot. The seal
        // is proven recoverable (enableVault round-trip-verified it before flipping on), so
        // dropping the leftover here is safe. Only fires in require-unlock mode (vault on +
        // sealed); Open-freely mode never has isOn() true, so this is a no-op there.
        if (IdentityVault.isOn(ctx) && IdentityVault.hasSealed(ctx) && hasIdentity(ctx)) {
            deleteIdentity(ctx)
        }
        val s = seed ?: unlockedSeed
        if (s == null && IdentityVault.isOn(ctx) && IdentityVault.hasSealed(ctx)) {
            // HEADLESS-VAULT: vault is ON and the seed is still sealed. Rather than
            // refuse, boot the runtime HEADLESS — the carrier meshes + buffers
            // sealed messages (generic notifications) until a biometric unlock
            // decrypts them. The seed never leaves the vault; the carrier runs off
            // the one-way-derived node key in carrier-identity.json.
            //   • blob present → headless boot (DEK first; the blob is sealed under it).
            //   • blob ABSENT  → device vaulted before headless-vault shipped: REFUSE
            //     so the UI forces an unlock, which boots Full and backfills the blob
            //     for next time. (Rust also fails closed here — never mints a fresh
            //     identity — as a safety net.)
            if (!carrierIdentityFile(ctx).exists()) return false
            val dek = StorageVault.dekBase64(ctx) ?: return false
            hey_set_storage_key(dek)
            // Sentinel (NOT "") so the runtime takes the never-generate headless
            // ladder. "" would be filtered to None → load_or_create → a fresh seed.
            hey_init(ctx.filesDir.absolutePath + "/hey", "", PORT, "hey-social", HEADLESS_BOOT)
            started = true
            return true
        }
        if (s != null) unlockedSeed = s
        // Install the hardware-wrapped storage DEK BEFORE the runtime touches disk,
        // so the identity seed, Double-Ratchet keys and conversations are encrypted
        // at rest (StrongBox/TEE-wrapped). Must precede hey_init.
        StorageVault.dekBase64(ctx)?.let { hey_set_storage_key(it) }
        hey_init(ctx.filesDir.absolutePath + "/hey", "", PORT, "hey-social", s ?: "")
        started = true
        return true
    }

    /** Has an identity been provisioned (created via "create new", restored, or
     *  vault-sealed)? Used so the BACKGROUND service never auto-creates a fresh
     *  identity before the user has chosen new-vs-restore (which would skip the
     *  welcome screen — a device-timing race). */
    fun isProvisioned(ctx: android.content.Context): Boolean =
        started || unlockedSeed != null || hasIdentity(ctx) || IdentityVault.hasSealed(ctx)

    /** Start the runtime ONLY if already provisioned — never create on a brand-new
     *  install. The UI's create/restore choice does the first creation. */
    fun ensureStartedIfProvisioned(ctx: android.content.Context): Boolean =
        if (isProvisioned(ctx)) ensureStarted(ctx) else false

    /** Provision the wallet for EVERY user at onboarding: publish receive addresses
     *  (so others can tip them) + mark the wallet set up, so the Wallet tab opens
     *  straight to the wallet and tips resolve from the start (no lazy "tap Wallet"). */
    fun provisionWallet(ctx: android.content.Context): Boolean {
        val ok = runCatching { publishTipAddresses(ctx) }.getOrDefault(false)
        ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE).edit()
            .putBoolean("elastos_setup", true).putBoolean("tips_published", ok).apply()
        return ok
    }

    /** Is this a valid 12/24-word BIP39 recovery phrase? (checked in Rust). */
    fun validMnemonic(phrase: String): Boolean = runCatching { hey_validate_mnemonic(phrase.trim()) == "ok" }.getOrDefault(false)

    // ── identity file (sealed at rest under the storage DEK; see StorageVault) ──
    fun identityFile(ctx: android.content.Context) = java.io.File(ctx.filesDir, "hey/identity.json")
    /** Headless-vault carrier blob (one-way node key + public did, sealed under the
     *  DEK). Its EXISTENCE — checked here — decides whether a vault-ON device can
     *  cold-start headless or must unlock first to backfill it. Never parsed in Kotlin. */
    fun carrierIdentityFile(ctx: android.content.Context) = java.io.File(ctx.filesDir, "hey/carrier-identity.json")
    /** Biometric unlock: hand the runtime the unsealed seed so a HEADLESS boot
     *  completes — IDENTITY installs, buffered messages decrypt + drain. Returns the
     *  native code: 0 ok / -2 WRONG ACCOUNT for this device / other <0 = not ready. */
    fun unlock(phrase: String): Int = runCatching { hey_unlock(phrase.trim()) }.getOrDefault(-1)
    /** Persist the headless carrier blob from the LIVE identity — call BEFORE
     *  deleting identity.json when enabling the vault, so the blob always exists
     *  before the seed-on-disk copy is removed (no fail-closed window). */
    fun persistCarrierIdentity(): Int = runCatching { hey_persist_carrier_identity() }.getOrDefault(-1)
    /** Existence only — the contents are encrypted, so never parse this in Kotlin. */
    fun hasIdentity(ctx: android.content.Context): Boolean =
        runCatching { identityFile(ctx).exists() }.getOrDefault(false)
    /** The runtime-held recovery phrase (in-memory; identity.json is encrypted). */
    fun recoveryPhrase(): String? =
        runCatching { hey_recovery_phrase().trim().ifBlank { null } }.getOrNull()
    /** Ask the runtime to persist its identity to identity.json, sealed under the
     *  storage DEK (never plaintext). Returns 0 on success. */
    fun persistIdentity(): Int = runCatching { hey_persist_identity() }.getOrDefault(-1)
    /** Two-key split: clear the DEK from the runtime (app backgrounded/locked). */
    fun lockStorage() { runCatching { hey_lock_storage() } }
    /** Drop all outstanding one-shot spend grants WITHOUT clearing the DEK — wired
     *  to the Option-A re-gate so a money confirm can't outlive a screen re-lock. */
    fun revokeSpends() { runCatching { hey_revoke_spends() } }
    /** Re-install the DEK after a biometric unlock so the runtime drains buffered msgs. */
    fun installStorageKey(ctx: android.content.Context) { runCatching { StorageVault.dekBase64(ctx)?.let { hey_set_storage_key(it) } } }
    fun inboundCount(): Long = runCatching { hey_inbound_count() }.getOrDefault(0L)
    /** True while the receiver defers processing — storage locked OR seed sealed
     *  (headless). The notifier uses this to post a generic "new message" from the
     *  no-DEK inbound counter so a headless device still notifies before unlock. */
    fun processingDeferred(): Boolean = runCatching { hey_processing_deferred() }.getOrDefault(false)
    fun readIdentity(ctx: android.content.Context): String? =
        runCatching { identityFile(ctx).takeIf { it.exists() }?.readText() }.getOrNull()
    // NOTE: no writeIdentity — Kotlin must NEVER author identity.json. Only the
    // runtime writes it (persistIdentity), as a well-formed IdentityBlob sealed
    // under the storage DEK. A raw write here risked a bare mnemonic that the
    // runtime can't parse → fresh-mint on cold start (silent account loss).
    fun deleteIdentity(ctx: android.content.Context) { runCatching { identityFile(ctx).delete() } }

    /** Wipe the decrypted-media staging dirs. Attachments/videos are decrypted to
     *  cache only to hand an external viewer a FileProvider URI; that E2E content
     *  must not linger in plaintext. Called on launch so it never outlives a
     *  session. (Not on background: an external viewer may still be reading it.) */
    fun clearDecryptedCache(ctx: android.content.Context) {
        runCatching {
            for (d in arrayOf("attachments", "media")) {
                java.io.File(ctx.cacheDir, d).listFiles()?.forEach { runCatching { it.delete() } }
            }
        }
    }

    // ── native (JNI) ────────────────────────────────────────────────────────
    /** Install the hardware-wrapped storage DEK (Base64) so the runtime encrypts
     *  all key material + data at rest. Call BEFORE hey_init. Returns 0 on success. */
    external fun hey_set_storage_key(dekB64: String): Int
    external fun hey_lock_storage()
    external fun hey_revoke_spends()
    external fun hey_inbound_count(): Long
    external fun hey_processing_deferred(): Boolean
    external fun hey_unlock(phrase: String): Int
    external fun hey_persist_carrier_identity(): Int
    external fun hey_init(dataDir: String, distDir: String, port: Int, capsule: String, identityBlob: String): Int
    /** Runtime-held BIP39 phrase (in-memory; identity.json is encrypted at rest). */
    external fun hey_recovery_phrase(): String
    /** H5: hardware-verified seed reveal — returns the mnemonic only after the Rust
     *  guard verifies a fresh Keystore signature over the reveal challenge. */
    external fun hey_recovery_phrase_hw(sigHex: String): String
    external fun hey_reveal_challenge(): String
    /** Persist the in-memory identity to identity.json sealed under the storage DEK. */
    external fun hey_persist_identity(): Int
    external fun hey_whoami(): String
    external fun hey_carrier_health(): String
    // DDRM (local-first, no chain): encrypt/store a .glb, and fetch/decrypt it on-device.
    external fun hey_ddrm_load(cid: String, ck: String): String
    external fun hey_ddrm_pack(glbPath: String, ck: String): String
    external fun hey_feed(limit: Int): String
    external fun hey_upload_media(bytes: ByteArray, mime: String, filename: String): String
    external fun hey_create_post(caption: String, mediaTilesJson: String): String
    external fun hey_react(postId: String, emoji: String): String
    external fun hey_get_reactions(postId: String): String
    external fun hey_add_comment(postId: String, text: String, parent: String): String
    external fun hey_get_comments(postId: String): String
    external fun hey_my_friend_link(): String
    external fun hey_follow(input: String): String
    external fun hey_unfollow(did: String): String
    external fun hey_following(): String
    external fun hey_is_following(did: String): String
    external fun hey_feed_rev(): Long
    // chat
    external fun hey_contacts(): String
    external fun hey_groups(): String
    external fun hey_conversation(did: String): String
    private external fun hey_edit_message(chatId: String, msgId: String, text: String, isGroup: Boolean): String
    external fun hey_group_conversation(gid: String): String
    external fun hey_send_dm(did: String, text: String): String
    external fun hey_call_send(did: String, payload: String): String
    external fun hey_call_poll(): String
    external fun hey_verse_send(did: String, payload: String): String
    external fun hey_verse_poll(): String
    external fun hey_net_changed()
    // Voice call audio (Stage 2)
    external fun hey_voice_start(peerTicket: String, isCaller: Boolean)
    external fun hey_voice_peers(): Int
    external fun hey_verse_rt_join(did: String)
    external fun hey_verse_rt_send(payload: String)
    external fun hey_verse_rt_recv(): String
    external fun hey_verse_rt_reset()
    external fun hey_verse_gossip_join(world: String, peerDids: String)
    external fun hey_verse_gossip_send(payload: String)
    external fun hey_verse_gossip_reset()
    external fun hey_voice_send(pcm: ByteArray)
    external fun hey_voice_recv(maxBytes: Int): ByteArray
    external fun hey_voice_set_muted(muted: Boolean)
    external fun hey_voice_stop()
    // Video calls (direct-only) — H.264 frames over QUIC uni-streams.
    external fun hey_video_start(peerTicket: String)
    external fun hey_video_send_frame(frame: ByteArray)
    external fun hey_video_recv_frame(): ByteArray
    external fun hey_video_set_paused(paused: Boolean)
    external fun hey_video_peers(): Int
    external fun hey_video_dropped(): Long
    external fun hey_video_stop()
    external fun hey_peer_ticket(did: String): String
    external fun hey_contact_transport(did: String): String
    external fun hey_attachment_progress(id: String): Int
    // Group voice calls (mesh)
    external fun hey_group_call_start(gid: String): String
    external fun hey_group_call_signal(gid: String, callId: String, kind: String): String
    external fun hey_group_call_roster(gid: String, callId: String): String
    external fun hey_group_active_call(gid: String): String
    external fun hey_voice_group_start()
    external fun hey_voice_sync(tickets: String)
    external fun hey_send_group(gid: String, text: String): String
    external fun hey_total_unread(): Int
    external fun hey_mark_read(did: String)
    external fun hey_group_mark_read(gid: String)
    /** Clear the unread badge for a chat (1:1 or group). */
    fun markRead(chat: Chat) = runCatching { if (chat.isGroup) hey_group_mark_read(chat.id) else hey_mark_read(chat.id) }
    external fun hey_accept_invite(token: String): String
    external fun hey_followers(): String
    external fun hey_follow_back(did: String): String
    external fun hey_start_chat(did: String): String
    external fun hey_user_posts(did: String): String
    external fun hey_delete_conversation(did: String): String
    external fun hey_delete_group(gid: String): String
    external fun hey_set_profile(nickname: String, bio: String, avatar: String): String
    external fun hey_get_profile(did: String): String
    external fun hey_delete_post(id: String): String
    external fun hey_edit_post(id: String, caption: String): String
    external fun hey_drain_notifs(): String
    // chat extras
    external fun hey_send_attachment(did: String, text: String, bytes: ByteArray, mime: String, filename: String): String
    external fun hey_send_group_attachment(gid: String, text: String, bytes: ByteArray, mime: String, filename: String): String
    external fun hey_fetch_attachment(attJson: String): ByteArray
    // Streamed (torrent-style) — pass a FILE PATH so big files never load whole into RAM.
    external fun hey_send_attachment_path(did: String, text: String, path: String, mime: String, filename: String): String
    external fun hey_send_group_attachment_path(gid: String, text: String, path: String, mime: String, filename: String): String
    external fun hey_fetch_attachment_to_path(attJson: String, destPath: String): String
    external fun hey_create_group(name: String, membersJson: String): String
    external fun hey_react_message(chatId: String, messageId: String, emoji: String, isGroup: Boolean): String
    external fun hey_delete_message(chatId: String, msgId: String, isGroup: Boolean): String
    external fun hey_message_reactions(chatId: String, isGroup: Boolean): String
    external fun hey_content_bytes(cid: String): ByteArray
    // wallet (EVM chains via elastos://<chain>/ — same mnemonic). Call off the main thread.
    // Wallet calls take a phrase ONLY as a legacy/pre-init fallback — "" means
    // "sign with the runtime-held identity" so the phrase never crosses the
    // bridge (guard.rs: secrets are used, never owned). Sends additionally
    // require a one-shot spend grant (hey_authorize_spend) or they refuse.
    external fun hey_wallet_address(mnemonic: String): String
    external fun hey_wallet_chains(): String
    external fun hey_wallet_balance(mnemonic: String, chain: String): String
    external fun hey_wallet_send(mnemonic: String, chain: String, to: String, valueHex: String, auth: String): String
    external fun hey_wallet_check_address(addr: String): String
    external fun hey_wallet_tx_status(chain: String, hash: String): String
    external fun hey_wallet_balances(mnemonic: String, chain: String): String
    external fun hey_wallet_token_send(mnemonic: String, chain: String, contract: String, to: String, amountHex: String, auth: String): String
    external fun hey_wallet_nfts(mnemonic: String, chain: String, added: String): String
    external fun hey_wallet_nft_lookup(mnemonic: String, chain: String, contract: String, tokenId: String): String
    external fun hey_wallet_nft_send_721(mnemonic: String, chain: String, contract: String, to: String, tokenId: String, auth: String): String
    external fun hey_wallet_nft_send_1155(mnemonic: String, chain: String, contract: String, to: String, tokenId: String, qty: String, auth: String): String
    // Self-host blockchain nodes: per-chain RPC override (default = bundled public RPC).
    external fun hey_set_rpc(chain: String, url: String): String
    external fun hey_rpc_nodes(): String
    // Elastos DID (EID) + ELA mainchain — same mnemonic, Essentials-recoverable.
    external fun hey_elastos_did(mnemonic: String): String
    external fun hey_ela_address(mnemonic: String): String
    external fun hey_ela_balance(mnemonic: String): String
    external fun hey_ela_send(mnemonic: String, to: String, amount: String, auth: String): String
    /** BEAM send UNDER THE GUARD (H1): redeem_spend + cap + in-process phrase + C++ invoke, all in Rust. */
    external fun hey_beam_send(to: String, amountBeam: String, amountGroth: Long, feeGroth: Long, assetId: Int, dir: String, node: String, auth: String): String
    external fun hey_beam_lift_cap()
    external fun hey_beam_reset_cap()
    /** sync-on-tip: true (once) when an incoming BEAM tip DM was just received → auto quick-sync. */
    external fun hey_beam_tip_pending(): Boolean
    // BEAM read ops — phrase resolved in-process by Rust (H5); mnemonic never crosses JNI from Kotlin.
    external fun hey_beam_address(dir: String): String
    external fun hey_beam_balance(dir: String, node: String): String
    external fun hey_beam_scan(dir: String, node: String): String
    external fun hey_beam_tx_status(dir: String, node: String, txid: String): String
    external fun hey_beam_node_start(dir: String): String
    external fun hey_beam_scan_local(dir: String, waitMs: Int): String
    // The law surface (guard.rs): one-shot spend grants + the user's own audit record.
    external fun hey_authorize_spend(kind: String, to: String, amount: String): String
    external fun hey_authorize_spend_fee(kind: String, to: String, amount: String, maxFeeWei: String): String
    external fun hey_authorize_spend_fee_hw(kind: String, to: String, amount: String, maxFeeWei: String, sigHex: String): String
    external fun hey_wallet_fee_estimate(mnemonic: String, chain: String, to: String, valueHex: String): String
    // Hardware-bound spend authorization (SpendAuth) — fail-safe, dormant until enrolled.
    external fun hey_enroll_spend_key(sec1B64: String): Int
    external fun hey_spend_selftest(sec1B64: String, challenge: String, sigHex: String): Int
    external fun hey_spend_challenge(): String
    external fun hey_authorize_spend_hw(kind: String, to: String, amount: String, sigHex: String): String
    external fun hey_unenroll_spend_key(): Int
    external fun hey_unenroll_challenge(): String
    external fun hey_unenroll_spend_key_hw(sigHex: String): Int
    // tipping: publish my receive addresses in my signed profile + resolve a peer's.
    external fun hey_set_tip_addresses(addressesJson: String): String
    external fun hey_resolve_tip(did: String): String
    external fun hey_refresh_contact(did: String): String
    external fun hey_notify_tip(to: String, sym: String, amount: String, txid: String): String
    external fun hey_validate_mnemonic(phrase: String): String

    // ── typed helpers ───────────────────────────────────────────────────────
    const val LIKE = "❤️"
    /** A user's media lives in their personal WebSpace drive (PC2 data plane),
     *  addressed by namespace — never by network. The runtime resolves it (see
     *  WebSpaceFetcher); the bytes come back through the elastos content provider. */
    fun mediaUri(cid: String) = "localhost://WebSpaces/hey/$cid"
    /** Resolve a WebSpace media handle (or bare cid) to bytes via the content provider. */
    fun contentBytes(uriOrCid: String): ByteArray {
        val cid = uriOrCid.substringAfterLast('/').trim()
        return runCatching { hey_content_bytes(cid) }.getOrDefault(ByteArray(0))
    }

    fun whoami(): JSONObject = JSONObject(hey_whoami())
    fun health(): JSONObject = JSONObject(hey_carrier_health())
    fun friendLink(): String = hey_my_friend_link()

    // DDRM: decrypt a stored .ddrm by cid → base64 .glb (null on error); encrypt+store a .glb → cid.
    fun ddrmLoadB64(cid: String, ck: String): String? =
        JSONObject(hey_ddrm_load(cid, ck)).let { if (it.has("b64")) it.getString("b64") else null }
    fun ddrmPack(glbB64: String, ck: String): String? =
        JSONObject(hey_ddrm_pack(glbB64, ck)).let { if (it.has("cid")) it.getString("cid") else null }

    fun feed(limit: Int = 50): List<Post> {
        val raw = hey_feed(limit)
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map { Post.from(arr.getJSONObject(it)) }
    }

    fun uploadMedia(bytes: ByteArray, mime: String, filename: String): JSONObject =
        JSONObject(hey_upload_media(bytes, mime, filename))

    fun createPost(caption: String, tiles: List<JSONObject>): JSONObject {
        val arr = JSONArray()
        tiles.forEach { arr.put(it) }
        return JSONObject(hey_create_post(caption, arr.toString()))
    }

    fun reactions(postId: String): Reactions = Reactions.from(JSONObject(hey_get_reactions(postId)))
    fun toggleLike(postId: String): Reactions = Reactions.from(JSONObject(hey_react(postId, LIKE)))

    fun comments(postId: String): List<Comment> {
        val raw = hey_get_comments(postId)
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map { Comment.from(arr.getJSONObject(it)) }
    }
    fun addComment(postId: String, text: String, parent: String = ""): Comment =
        Comment.from(JSONObject(hey_add_comment(postId, text, parent)))
    fun setProfile(nickname: String, bio: String, avatar: String): Boolean =
        !JSONObject(hey_set_profile(nickname, bio, avatar)).has("error")
    fun profile(did: String = ""): Profile = Profile.from(JSONObject(hey_get_profile(did)))
    fun deletePost(id: String): Boolean = !JSONObject(hey_delete_post(id)).has("error")
    fun editPost(id: String, caption: String): Boolean = !JSONObject(hey_edit_post(id, caption)).has("error")

    // ── chat helpers ─────────────────────────────────────────────────────────
    fun chats(): List<Chat> {
        val out = ArrayList<Chat>()
        runCatching {
            val c = JSONArray(hey_contacts())
            for (i in 0 until c.length()) {
                val o = c.getJSONObject(i)
                val did = o.optString("did")
                val pv = o.optString("lastPreview").let { if (isProtocolText(it)) "" else it }
                out.add(Chat(did, o.optString("name").ifBlank { shortDid(did) },
                    pv, o.optLong("lastTs"), o.optInt("unread"), false, o.optString("avatar")))
            }
        }
        runCatching {
            val g = JSONArray(hey_groups())
            for (i in 0 until g.length()) {
                val o = g.getJSONObject(i)
                val members = o.optJSONArray("members")?.length() ?: 0
                out.add(Chat(o.optString("id"), o.optString("name").ifBlank { "Group" },
                    "$members members", o.optLong("lastTs"), o.optInt("unread"), true))
            }
        }
        return out.sortedByDescending { it.ts }
    }

    /** Protocol/handshake payloads (verse invites, call/edit/del signals) must
     *  never RENDER as chat text — this also hides rows older builds already
     *  stored in the conversation db. */
    fun isProtocolText(t: String): Boolean {
        val s = t.removePrefix("\u0001")
        return s.startsWith("hey-verse:1:") || s.startsWith("hey-addr:1:") ||
            s.startsWith("hey-call:1:") || s.startsWith("hey-del:1:") ||
            s.startsWith("hey-edit:1:") || s.startsWith("hey-gcall:1:")
    }

    fun conversation(chat: Chat): List<Msg> {
        val raw = if (chat.isGroup) hey_group_conversation(chat.id) else hey_conversation(chat.id)
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map {
            val o = arr.getJSONObject(it)
            val atts = o.optJSONArray("attachments")?.let { a ->
                (0 until a.length()).map { i ->
                    val ao = a.getJSONObject(i)
                    Attachment(ao.optString("name"), ao.optString("mime"), ao.optLong("size"), ao.toString())
                }
            } ?: emptyList()
            Msg(o.optString("id"), o.optString("text"), o.optLong("ts"), o.optBoolean("mine"), o.optString("sender_name"), atts)
        }.filter { it.attachments.isNotEmpty() || !isProtocolText(it.text) }
    }
    /** Edit one of your own messages, for everyone in the chat. */
    fun editMessage(chat: Chat, id: String, text: String): Boolean =
        runCatching { JSONObject(hey_edit_message(chat.id, id, text, chat.isGroup)).optBoolean("ok", false) }
            .getOrDefault(false)

    fun send(chat: Chat, text: String): Boolean {
        val r = if (chat.isGroup) hey_send_group(chat.id, text) else hey_send_dm(chat.id, text)
        return !JSONObject(r).has("error")
    }

    // ── 1:1 voice call signaling (Stage 1: setup; audio is Stage 2) ───────────
    /** Send a call-control signal (offer/accept/decline/end) to a contact over the E2E DM channel. */
    fun callSend(did: String, payload: JSONObject): Boolean =
        runCatching { JSONObject(hey_call_send(did, payload.toString())).optBoolean("ok", false) }.getOrDefault(false)

    /** The peer's carrier ticket for dialing a voice call (empty if unknown). Off-main. */
    fun peerTicket(did: String): String = runCatching { hey_peer_ticket(did) }.getOrDefault("")
    /** Live transport to a contact: "direct" | "relay" | "offline". Off-main. */
    fun contactTransport(did: String): String = runCatching { hey_contact_transport(did) }.getOrDefault("offline")
    /** Download progress 0..100 for an in-flight attachment fetch, -1 if not active. */
    fun attachmentProgress(id: String): Int = runCatching { hey_attachment_progress(id) }.getOrDefault(-1)
    fun voiceStart(peerTicket: String, isCaller: Boolean) = runCatching { hey_voice_start(peerTicket, isCaller) }
    /** Live voice links in the current call — 0 means the audio dial hasn't landed yet. */
    fun voicePeers(): Int = runCatching { hey_voice_peers() }.getOrDefault(0)
    // ── verse REALTIME lane (movement over QUIC datagrams; DMs only as fallback) ──
    fun verseRtJoin(did: String) { runCatching { hey_verse_rt_join(did) } }
    fun verseRtSend(payload: String) { runCatching { hey_verse_rt_send(payload) } }
    fun verseRtReset() { runCatching { hey_verse_rt_reset() } }
    fun verseGossipJoin(world: String, dids: List<String>) { runCatching { hey_verse_gossip_join(world, dids.joinToString("\n")) } }
    fun verseGossipSend(payload: String) { runCatching { hey_verse_gossip_send(payload) } }
    fun verseGossipReset() { runCatching { hey_verse_gossip_reset() } }
    fun verseRtPoll(): List<Pair<String, JSONObject>> = runCatching {
        val arr = JSONArray(hey_verse_rt_recv())
        (0 until arr.length()).mapNotNull { i ->
            val o = arr.getJSONObject(i)
            val p = o.optJSONObject("payload") ?: return@mapNotNull null
            o.optString("from") to p
        }
    }.getOrDefault(emptyList())
    fun voiceSend(pcm: ByteArray) { runCatching { hey_voice_send(pcm) } }
    fun voiceRecv(maxBytes: Int): ByteArray = runCatching { hey_voice_recv(maxBytes) }.getOrDefault(ByteArray(0))
    fun voiceSetMuted(muted: Boolean) { runCatching { hey_voice_set_muted(muted) } }
    // Video calls (direct-only) — stage 1 transport surface; camera/codec/UI in later stages.
    fun videoStart(peerTicket: String) { runCatching { hey_video_start(peerTicket) } }
    fun videoSendFrame(frame: ByteArray) { runCatching { hey_video_send_frame(frame) } }
    fun videoRecvFrame(): ByteArray = runCatching { hey_video_recv_frame() }.getOrDefault(ByteArray(0))
    fun videoSetPaused(paused: Boolean) { runCatching { hey_video_set_paused(paused) } }
    fun videoPeers(): Int = runCatching { hey_video_peers() }.getOrDefault(0)
    fun videoDropped(): Long = runCatching { hey_video_dropped() }.getOrDefault(0L)
    fun videoStop() { runCatching { hey_video_stop() } }
    fun voiceStop() { runCatching { hey_voice_stop() } }

    // ── group voice calls (mesh) ──────────────────────────────────────────────
    /** Announce a group call → {ok, call_id, ticket}. Off-main. */
    fun groupCallStart(gid: String): JSONObject = runCatching { JSONObject(hey_group_call_start(gid)) }.getOrDefault(JSONObject())
    /** Emit a control signal: kind = "join" | "leave" | "end". */
    fun groupCallSignal(gid: String, callId: String, kind: String): Boolean =
        runCatching { JSONObject(hey_group_call_signal(gid, callId, kind)).optBoolean("ok", false) }.getOrDefault(false)
    /** Live call state → {active, ended, participants:[{did,ticket,name,mine}]}. */
    fun groupCallRoster(gid: String, callId: String): JSONObject =
        runCatching { JSONObject(hey_group_call_roster(gid, callId)) }.getOrDefault(JSONObject())
    /** The latest joinable call on a group thread → {active, call_id, participants[…]}. */
    fun groupActiveCall(gid: String): JSONObject =
        runCatching { JSONObject(hey_group_active_call(gid)) }.getOrDefault(JSONObject())
    /** Open the (empty) group-call audio mesh; peers join as the roster syncs. */
    fun voiceGroupStart() { runCatching { hey_voice_group_start() } }
    /** Reconcile the mesh to these participant tickets (newline-joined for the JNI boundary). */
    fun voiceSync(tickets: List<String>) { runCatching { hey_voice_sync(tickets.joinToString("\n")) } }

    // ── Hey Verse lane: ephemeral world presence. Sealed like a DM on the
    // wire but NEVER stored/unread/notified — and it can't drown calls. ─────
    fun verseSend(did: String, payload: JSONObject): Boolean =
        runCatching { JSONObject(hey_verse_send(did, payload.toString())).optBoolean("ok", false) }.getOrDefault(false)

    /** Drain the verse inbox → (from did, payload) pairs. Single consumer! */
    fun versePoll(): List<Pair<String, JSONObject>> = runCatching {
        val arr = JSONArray(hey_verse_poll())
        (0 until arr.length()).mapNotNull {
            val o = arr.getJSONObject(it)
            val p = o.optJSONObject("payload") ?: return@mapNotNull null
            o.optString("from") to p
        }
    }.getOrDefault(emptyList())

    /** Poll for inbound call signals (each delivered once). Call ~1s while the app is open. */
    fun callPoll(): List<CallSignal> = runCatching {
        val arr = JSONArray(hey_call_poll())
        (0 until arr.length()).mapNotNull {
            val o = arr.getJSONObject(it)
            val p = o.optJSONObject("payload") ?: return@mapNotNull null
            CallSignal(o.optString("from"), p.optString("type"), p.optString("call_id"), p)
        }
    }.getOrDefault(emptyList())
    /** Send one file/photo in a chat. Returns null on success, else an error string. */
    fun sendAttachment(chat: Chat, bytes: ByteArray, mime: String, filename: String, text: String = ""): String? {
        val r = if (chat.isGroup) hey_send_group_attachment(chat.id, text, bytes, mime, filename)
                else hey_send_attachment(chat.id, text, bytes, mime, filename)
        val o = JSONObject(r); return if (o.has("error")) o.getString("error") else null
    }
    fun fetchAttachment(att: Attachment): ByteArray = runCatching { hey_fetch_attachment(att.raw) }.getOrDefault(ByteArray(0))
    /** Streamed send from a file PATH (big files, O(chunk) RAM). Returns null on success, else error. */
    fun sendAttachmentPath(chat: Chat, path: String, mime: String, filename: String, text: String = ""): String? {
        val r = if (chat.isGroup) hey_send_group_attachment_path(chat.id, text, path, mime, filename)
                else hey_send_attachment_path(chat.id, text, path, mime, filename)
        val o = JSONObject(r); return if (o.has("error")) o.getString("error") else null
    }
    /** Streamed fetch: download + decrypt straight to `dest` on disk. Result.success(dest) or failure(reason). */
    fun fetchAttachmentToPath(att: Attachment, dest: java.io.File): Result<java.io.File> = runCatching {
        val r = hey_fetch_attachment_to_path(att.raw, dest.absolutePath)
        val o = JSONObject(r)
        if (o.has("error")) throw IllegalStateException(o.getString("error"))
        dest
    }
    fun createGroup(name: String, memberDids: List<String>): String? {
        val arr = JSONArray(); memberDids.forEach { arr.put(it) }
        val o = JSONObject(hey_create_group(name, arr.toString()))
        return if (o.has("error")) null else o.optString("id")
    }
    fun reactToMessage(chat: Chat, msgId: String, emoji: String): Boolean =
        !JSONObject(hey_react_message(chat.id, msgId, emoji, chat.isGroup)).has("error")
    /** Delete one of my own messages for everyone (tombstone over the E2E channel). */
    fun deleteMessage(chat: Chat, msgId: String): Boolean =
        runCatching { JSONObject(hey_delete_message(chat.id, msgId, chat.isGroup)).optBoolean("ok", false) }.getOrDefault(false)
    /** message_id -> reactions on it. */
    fun messageReactions(chat: Chat): Map<String, List<MsgReaction>> {
        val raw = hey_message_reactions(chat.id, chat.isGroup)
        if (!raw.trimStart().startsWith("[")) return emptyMap()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map {
            val o = arr.getJSONObject(it)
            MsgReaction(o.optString("message_id"), o.optString("emoji"), o.optString("sender_did"))
        }.groupBy { it.messageId }
    }
    fun acceptInvite(token: String): JSONObject = JSONObject(hey_accept_invite(token))
    fun shortDid(did: String) = did.removePrefix("did:key:z").take(10) + "…"

    fun follow(input: String): JSONObject = JSONObject(hey_follow(input))
    fun followers(): List<Follow> {
        val raw = hey_followers()
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map { val o = arr.getJSONObject(it); Follow(o.optString("did"), o.optString("ticket")) }
    }
    fun followBack(did: String): Boolean = !JSONObject(hey_follow_back(did)).has("error")
    /** Ensure a DM contact for did; returns error string or null on success. */
    fun startChat(did: String): String? {
        val o = JSONObject(hey_start_chat(did)); return if (o.has("error")) o.getString("error") else null
    }
    fun userPosts(did: String): List<Post> {
        val raw = hey_user_posts(did)
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map { Post.from(arr.getJSONObject(it)) }
    }
    fun deleteChat(chat: Chat) { if (chat.isGroup) hey_delete_group(chat.id) else hey_delete_conversation(chat.id) }

    // ── per-chat local prefs (mute / block) ──────────────────────────────────
    private fun heyPrefs(ctx: android.content.Context) = ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE)
    private fun prefSet(ctx: android.content.Context, key: String): Set<String> = heyPrefs(ctx).getStringSet(key, emptySet()) ?: emptySet()
    private fun prefToggle(ctx: android.content.Context, key: String, value: String, on: Boolean) {
        val s = HashSet(prefSet(ctx, key)); if (on) s.add(value) else s.remove(value)
        heyPrefs(ctx).edit().putStringSet(key, s).apply()
    }
    fun isChatMuted(ctx: android.content.Context, did: String) = did in prefSet(ctx, "muted_chats")
    fun setChatMuted(ctx: android.content.Context, did: String, muted: Boolean) = prefToggle(ctx, "muted_chats", did, muted)
    fun blockedDids(ctx: android.content.Context): Set<String> = prefSet(ctx, "blocked_dids")
    fun isBlocked(ctx: android.content.Context, did: String) = did in prefSet(ctx, "blocked_dids")
    fun setBlocked(ctx: android.content.Context, did: String, blocked: Boolean) = prefToggle(ctx, "blocked_dids", did, blocked)
    fun dismissedNotifs(ctx: android.content.Context): Set<String> = prefSet(ctx, "dismissed_notifs")
    fun setNotifDismissed(ctx: android.content.Context, did: String, dismissed: Boolean) = prefToggle(ctx, "dismissed_notifs", did, dismissed)
    fun drainNotifs(): List<JSONObject> {
        val raw = hey_drain_notifs()
        if (!raw.trimStart().startsWith("[")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map { arr.getJSONObject(it) }
    }
    fun unfollow(did: String): JSONObject = JSONObject(hey_unfollow(did))
    fun following(): List<Follow> {
        val raw = hey_following()
        if (raw.trimStart().startsWith("{")) return emptyList()
        val arr = JSONArray(raw)
        return (0 until arr.length()).map {
            val o = arr.getJSONObject(it); Follow(o.optString("did"), o.optString("ticket"))
        }
    }

    // ── wallet (ESC) ──────────────────────────────────────────────────────────
    /** The BIP39 recovery phrase. Sourced from the in-memory seed (vault unlock)
     *  or, once the runtime is up, the runtime itself (hey_recovery_phrase) —
     *  identity.json is encrypted at rest now, so we never parse it in Kotlin
     *  except as a legacy-plaintext fallback for a not-yet-migrated install. */
    fun mnemonic(ctx: android.content.Context): String? {
        fun parse(s: String): String? = s.trim().let {
            if (it.startsWith("{")) runCatching { JSONObject(it).optString("mnemonic").ifBlank { null } }.getOrNull()
            else it.ifBlank { null }
        }
        unlockedSeed?.let { parse(it)?.let { m -> return m } }
        if (started) recoveryPhrase()?.let { return it }
        return readIdentity(ctx)?.let { parse(it) } // legacy plaintext only
    }
    /** Wallet ops sign with the RUNTIME-HELD identity: "" tells Rust to resolve
     *  the recovery phrase in-process (guard.rs: secrets are used, never owned),
     *  so the phrase stops crossing the JNI bridge per call. Before the runtime
     *  is up we fall back to the explicit phrase (provisioning previews). */
    private fun phraseArg(ctx: android.content.Context): String =
        if (started) "" else (mnemonic(ctx) ?: "")
    /** Same ESC address you'd recover in official Elastos Essentials. */
    fun walletAddress(ctx: android.content.Context): String? =
        hey_wallet_address(phraseArg(ctx)).ifBlank { null }
    /** Native Elastos DID (EID), byte-for-byte the same as Essentials. Instant + local. */
    fun elastosDid(ctx: android.content.Context): String? =
        hey_elastos_did(phraseArg(ctx)).ifBlank { null }
    /** ELA mainchain "E…" address (same mnemonic). */
    fun elaAddress(ctx: android.content.Context): String? =
        hey_ela_address(phraseArg(ctx)).ifBlank { null }

    // ── spend grants (guard.rs) ──────────────────────────────────────────────
    // The user's confirmation mints a ONE-SHOT authorization bound to exactly
    // (kind, recipient, amount) with a 90s TTL; the Rust signer refuses to sign
    // without redeeming one. Mint ONLY from the confirm dialog (behind the
    // biometric gate) — never wholesale.
    private fun mintSpend(kind: String, to: String, amount: String): String =
        runCatching { JSONObject(hey_authorize_spend(kind, to, amount)).optString("token") }.getOrDefault("")
    private fun mintSpendFee(kind: String, to: String, amount: String, maxFeeWei: String): String =
        runCatching { JSONObject(hey_authorize_spend_fee(kind, to, amount, maxFeeWei)).optString("token") }.getOrDefault("")
    /** Estimated MAX native fee for a send to `to` (decimal `amount`) on `chain`:
     *  {maxFeeWei, maxFee, gasPriceWei, gasLimit, symbol}, or null on error. Uses the
     *  SAME eth_estimateGas the signer uses (so a CONTRACT recipient won't fail the
     *  send closed via the max-fee bound — M-1). Shown on the confirm dialog and bound
     *  into the EVM spend grant. */
    fun feeEstimate(ctx: android.content.Context, chain: String, to: String, amount: String): JSONObject? = runCatching {
        val wei = toWeiHex(amount) ?: return null
        val o = JSONObject(hey_wallet_fee_estimate(phraseArg(ctx), chain, to.trim(), wei)); if (o.has("error")) null else o
    }.getOrNull()
    /** EVM native send grant. When `maxFeeWei` is non-empty the grant binds the fee
     *  (the Rust signer refuses a tx whose real fee exceeds it). */
    fun authorizeEvmSend(chain: String, to: String, amount: String): String =
        toWeiHex(amount)?.let { mintSpend("evm:$chain", to.trim(), it) } ?: ""
    fun authorizeEvmSendFee(chain: String, to: String, amount: String, maxFeeWei: String): String =
        toWeiHex(amount)?.let { mintSpendFee("evm:$chain", to.trim(), it, maxFeeWei) } ?: ""
    fun authorizeTokenSend(chain: String, contract: String, to: String, amount: String, decimals: Int): String =
        toUnitsHex(amount, decimals)?.let { mintSpend("erc20:$chain:$contract", to.trim(), it) } ?: ""
    fun authorizeElaSend(to: String, amount: String): String =
        mintSpend("ela", to.trim(), amount.trim())
    /** BEAM/BEAMX spend grant (H1). kind = "beam:<asset>"; amount = decimal BEAM verbatim
     *  (the Rust guard redeems the SAME (kind,to,amount), and hey_beam_send binds it). */
    fun authorizeBeamSend(asset: Int, to: String, amount: String): String =
        mintSpend("beam:$asset", to.trim(), amount.trim())
    /** NFT spend grants. The canonical (kind,amount) MUST match the Rust redeem
     *  byte-for-byte (literal compare): the amount is the DECIMAL token_id verbatim
     *  on both sides; 1155 binds the quantity into the kind so confirming "send #5"
     *  can't move a different count. */
    fun authorizeNftSend721(chain: String, contract: String, to: String, tokenIdDec: String): String =
        mintSpend("nft:$chain:$contract", to.trim(), tokenIdDec.trim())
    fun authorizeNftSend1155(chain: String, contract: String, to: String, tokenIdDec: String, qty: String): String =
        mintSpend("nft1155:$chain:$contract:${qty.trim()}", to.trim(), tokenIdDec.trim())
    /** BEAM static public_offline DONATION token — the one stable address tipping uses (reusable,
     *  never expires). Minted LOCALLY via libbeam.so once, then cached so it never changes.
     *  Null if BEAM isn't in this build or the mint fails. Call off the main thread. */
    fun beamAddress(ctx: android.content.Context): String? {
        if (!BeamApi.available) return null
        val prefs = ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE)
        prefs.getString("beam_tip_token", null)?.takeIf { it.isNotBlank() }?.let { return it }
        val token = BeamApi.address(BeamApi.beamDir(ctx)).getOrNull() ?: return null
        prefs.edit().putString("beam_tip_token", token).apply()
        return token
    }
    /** BEAM + BEAMX balances from the last sync (no network). Null if unavailable. Off-main. */
    fun beamBalance(ctx: android.content.Context): BeamBalance? {
        if (!BeamApi.available) return null
        return BeamApi.balance(BeamApi.beamDir(ctx)).getOrNull()
    }
    /** Connect to a node + sync so balances update. Blocks — call OFF the main thread. Self-host starts
     *  an on-device beam::Node (first sync = minutes); quicksync uses a public node. Returns the outcome
     *  (synced / still-syncing / real error) so the UI can show progress instead of a generic failure. */
    fun beamScan(ctx: android.content.Context): BeamScanResult {
        if (!BeamApi.available) return BeamScanResult(false, false, 0L, "BEAM not in this build")
        val node = beamNode(ctx)   // user-overridable (hey/beam-node.txt); else the public node
        return BeamApi.scan(BeamApi.beamDir(ctx), node)
    }
    /** Live self-host sync snapshot (block height + %) — poll ~1s while a sync runs. */
    fun beamSyncProgress(): BeamSyncProgress = BeamApi.syncProgress()

    // The BEAM node sync runs on a PROCESS-scoped coroutine (NOT the wallet UI), so it keeps syncing
    // when the wallet sheet closes or the app is backgrounded — the foreground RuntimeService keeps the
    // process alive. The node DB persists, so a kill just pauses it (resumes from disk, never from zero).
    private val beamScope = kotlinx.coroutines.CoroutineScope(kotlinx.coroutines.Dispatchers.IO + kotlinx.coroutines.SupervisorJob())
    @Volatile private var beamSyncing = false
    /** Last quick-sync failure (node down, port blocked) — the sheet shows it. */
    @Volatile var beamSyncError: String? = null
    /** Staged mobile-node status for the UI: "Starting node…" → "Connecting to peers…" → "Syncing N%"
     *  → "Synced" (B3). The node KEEPS RUNNING through all of these. Null when not in mobilenode. */
    @Volatile var beamNodeStage: String? = null
    /** NON-BLOCKING hint shown only if the mobile node still can't reach peers after ~45-60s (B3).
     *  Advisory only — the node is NOT stopped; it keeps retrying all seeds. Null = no hint. */
    @Volatile var beamNodeHint: String? = null
    /** Start (or no-op if already running) the BEAM sync. Branches on beamNodeMode:
     *   - "mobilenode": start the in-process node (loopback) + scan it; first mainnet sync can take
     *     HOURS, so a "still syncing" result is NOT an error — the watchdog keeps polling.
     *   - else (quicksync / ownnode): FlyClient scan against a public/own node.
     *  Idempotent at two layers: this flag AND the shim's single-reactor/single-node guard (W3). */
    fun beamSyncStart(ctx: android.content.Context) {
        if (beamSyncing || !BeamApi.available) return
        beamSyncing = true
        beamScope.launch {
            try {
                val dir = BeamApi.beamDir(ctx)
                when (beamNodeMode(ctx)) {
                    "mobilenode" -> {
                        beamSyncError = null; beamNodeHint = null
                        beamNodeStage = "Starting node…"
                        // B3 FIX: start the node and NEVER hard-fail on the reachability probe. The shim
                        // always starts the node (it owns a resilient retry loop over all seeds); a probe
                        // failure is only a NON-FATAL hint (nodeStatus().peersReachable). We surface a
                        // gentle, non-blocking hint after a grace period if peers still aren't reachable —
                        // but we KEEP THE NODE RUNNING and keep polling. We never auto-stop it.
                        if (!BeamApi.nodeStatus().running) {
                            // Surface the REAL init error (wallet locked / no kdf / db open failed)
                            // instead of a generic message — only a genuine error lands here now (not a
                            // blocked port). This tells the user (and us) exactly what to fix.
                            val e = BeamApi.nodeStartError(dir)
                            if (e.isNotEmpty()) {
                                beamNodeStage = null
                                beamSyncError = "Mobile node: $e"
                                return@launch
                            }
                        }
                        beamNodeStage = "Connecting to peers…"
                        // Watchdog loop: scan_local blocks up to ~60s waiting for node-synced. A
                        // node_syncing result means "still catching up" (B1) — keep polling, never an
                        // error. First mainnet sync can take HOURS; we only show a NON-BLOCKING hint
                        // (never stop the node) once it's been ~45-60s with NO first block AND the
                        // reachability probe said no seed answered.
                        var lastDone = 0L; var noBlockPolls = 0
                        while (true) {
                            val r = BeamApi.scanLocal(dir, 60_000)
                            when {
                                r.synced -> { beamNodeStage = "Synced"; beamNodeHint = null; beamSyncError = null; break }
                                r.nodeSyncing -> {
                                    val st = BeamApi.nodeStatus()
                                    if (st.done > lastDone) { lastDone = st.done; noBlockPolls = 0 } else noBlockPolls++
                                    beamSyncError = null
                                    // Stage: once blocks flow we're "Syncing"; before that we're "Connecting".
                                    beamNodeStage = if (st.total > 0L)
                                        "Syncing ${((st.done.coerceAtMost(st.total) * 100L) / st.total).toInt()}%"
                                        else "Connecting to peers…"
                                    // Non-blocking hint: first scanLocal already waited ~60s, so by the 1st
                                    // no-block poll we're well past 45-60s. Show the advisory ONLY when still
                                    // no block AND no seed was reachable; clear it the moment blocks arrive.
                                    beamNodeHint = if (st.done == 0L && st.total == 0L && !st.peersReachable && noBlockPolls >= 1)
                                        "Still reaching BEAM peers — make sure you're on Wi-Fi, or use Quick sync. (The node keeps trying.)"
                                        else null
                                    // keep polling — the node KEEPS RUNNING regardless of the hint
                                }
                                else -> { beamNodeStage = null; beamSyncError = r.error; break }   // a real failure
                            }
                        }
                    }
                    else -> {
                        beamNodeStage = null; beamNodeHint = null   // not mobile-node staging
                        // quicksync / ownnode (FlyClient). User's own node first, then the CURRENT
                        // official seeds (W1) — one dead host / filtered port no longer ends the story.
                        val candidates = linkedSetOf(
                            beamNode(ctx),
                            "eu-nodes.mainnet.beam.mw:8100",
                            "us-nodes.mainnet.beam.mw:8100",
                        )
                        var last: BeamScanResult? = null
                        for (n in candidates) {
                            val r = BeamApi.scan(dir, n)
                            last = r
                            if (r.synced) break
                        }
                        beamSyncError = if (last?.synced == true) null else last?.error
                    }
                }
            } finally { beamSyncing = false }
        }
    }
    /** Stop the on-device node off the main thread (the native dtor JOINS the node thread — W4).
     *  Call on logout / mode switch away from "mobilenode". W3: wait for any in-flight scan to finish
     *  before tearing the node down, so we never pull the node out from under an active loopback scan. */
    fun beamNodeStop() {
        beamScope.launch {
            var guard = 0
            while (beamSyncing && guard < 600) { kotlinx.coroutines.delay(500); guard++ }  // up to ~5 min
            BeamApi.nodeStop()
        }
    }
    /** Send BEAM (asset 0) or BEAMX (asset 7) UNDER THE GUARD (H1). amount = decimal BEAM.
     *  `auth` = the one-shot spend grant the user confirmed (kind="beam:<asset>", to=token,
     *  amount=decimal BEAM); Rust redeems it + enforces the cap + resolves the phrase in-process
     *  + invokes libbeam — the mnemonic no longer crosses JNI from Kotlin. OFF-main. */
    fun beamSend(ctx: android.content.Context, token: String, amount: String, asset: Int, auth: String): Result<BeamSendResult> {
        if (!BeamApi.available) return Result.failure(IllegalStateException("BEAM not in this build"))
        if (auth.isBlank()) return Result.failure(IllegalStateException("Confirm the transfer first"))
        return BeamApi.send(BeamApi.beamDir(ctx), token.trim(), amount.trim(), asset, auth, nodeUri = beamNode(ctx))
    }
    fun beamTxStatus(ctx: android.content.Context, txid: String): String {
        return BeamApi.txStatus(BeamApi.beamDir(ctx), txid, beamNode(ctx))
    }
    fun beamValidToken(token: String): Boolean = BeamApi.validToken(token)
    fun beamCapLifted(ctx: android.content.Context): Boolean = BeamApi.capLifted(ctx)
    fun setBeamCapLifted(ctx: android.content.Context, v: Boolean) = BeamApi.setCapLifted(ctx, v)

    // ── relay / "Hey mesh hub" selection ──────────────────────────────────────
    // The carrier reads dir/relay-url.txt at startup:
    //   missing/blank → STANDARD (default): iroh's stable production relays
    //   https URL     → that relay (federation or self-hosted), iroh as fallback
    // Devices on different relays still reach each other — each one is reachable
    // through its OWN home relay. Takes effect on the next app start.
    /** The Hey federation relay — written as the custom URL when the user picks "Federation". */
    const val RELAY_FEDERATED_URL = "https://elastos.app"
    private fun relayFile(ctx: android.content.Context) = java.io.File(ctx.filesDir, "hey/relay-url.txt")
    /** Raw relay choice: "" = standard (default) or a custom URL. Migrates the
     *  legacy "federated" keyword (older builds) to the URL it stood for. */
    fun customRelay(ctx: android.content.Context): String {
        val raw = runCatching { relayFile(ctx).takeIf { it.exists() }?.readText()?.trim() }.getOrNull().orEmpty()
        return if (raw.equals("federated", ignoreCase = true)) RELAY_FEDERATED_URL else raw
    }
    fun setCustomRelay(ctx: android.content.Context, url: String) {
        runCatching {
            val f = relayFile(ctx); f.parentFile?.mkdirs()
            val u = url.trim()
            if (u.isEmpty()) f.delete() else f.writeText(u)
        }
    }

    // ── BEAM node selection (privacy) ─────────────────────────────────────────
    // BEAM's node protocol is a raw TCP sync, not HTTPS — so the node operator
    // sees your IP + wallet activity. Default is a public node; a privacy-conscious
    // user can point at THEIR OWN node (self-host) so nothing leaks to a third
    // party. Threaded into beamScan/beamSyncStart/beamSend/beamTxStatus.
    private fun beamNodeFile(ctx: android.content.Context) = java.io.File(ctx.filesDir, "hey/beam-node.txt")
    /** The user's custom BEAM node (host:port), or the public default if unset. */
    fun beamNode(ctx: android.content.Context): String =
        runCatching { beamNodeFile(ctx).takeIf { it.exists() }?.readText()?.trim()?.ifBlank { null } }
            .getOrNull() ?: BeamApi.DEFAULT_NODE
    fun setBeamNode(ctx: android.content.Context, uri: String) {
        runCatching {
            val f = beamNodeFile(ctx); f.parentFile?.mkdirs()
            val u = uri.trim()
            if (u.isEmpty()) f.delete() else f.writeText(u)
        }
    }

    // ── Self-host blockchain (RPC) nodes ──────────────────────────────────────
    // Default is the bundled public Elastos RPC; a user can point ANY chain at
    // their OWN node (privacy / sovereignty). Persisted in Rust as <chain>-rpc.txt
    // and read by wallet::rpc_override on every balance/send. Empty = revert default.
    /** Set (or clear with an empty url) the self-host RPC node for `chain`
     *  ("esc"/"eid"/"ethereum"/"ela"). Returns the Rust JSON result. */
    fun setRpcNode(chain: String, url: String): String =
        runCatching { hey_set_rpc(chain, url) }.getOrDefault("{\"ok\":false}")
    /** Self-hostable chains: `[{key,name,default,override}]` (override "" = on default). */
    fun rpcNodes(): org.json.JSONArray =
        runCatching { org.json.JSONArray(hey_rpc_nodes()) }.getOrDefault(org.json.JSONArray())
    /** The (possibly user-overridden) IPFS gateway base for NFT media, always
     *  ending in "/". Read from the same self-host rows as the RPC/index nodes. */
    fun ipfsGateway(): String {
        val arr = rpcNodes()
        for (i in 0 until arr.length()) {
            val o = arr.getJSONObject(i)
            if (o.optString("key") == "ipfs-gateway") {
                val v = o.optString("override").ifBlank { o.optString("default") }
                return if (v.endsWith("/")) v else "$v/"
            }
        }
        return "https://ipfs.io/ipfs/"
    }
    /** Resolve an ipfs:// (or bare ipfs/CID) URL to an http(s) gateway URL; pass
     *  through anything else. Routes external NFT art to a SELF-HOSTABLE gateway
     *  instead of the in-process content store (which only holds OUR media). */
    fun resolveIpfs(raw: String): String {
        val u = raw.trim()
        val rest = when {
            u.startsWith("ipfs://ipfs/") -> u.removePrefix("ipfs://ipfs/")
            u.startsWith("ipfs://") -> u.removePrefix("ipfs://")
            u.startsWith("ipfs/") -> u.removePrefix("ipfs/")
            else -> return u
        }
        return ipfsGateway() + rest
    }

    /** ELA mainchain balance (decimal ELA string), or null on error. */
    fun elaBalance(ctx: android.content.Context): String? {
        val o = JSONObject(hey_ela_balance(phraseArg(ctx)))
        return if (o.has("error")) null else o.optString("ela")
    }
    /** Send ELA on the mainchain (UTXO). amount = decimal ELA string.
     *  `auth` = the spend grant from authorizeElaSend — without it Rust refuses. */
    fun elaSend(ctx: android.content.Context, to: String, amount: String, auth: String): Result<String> {
        val o = JSONObject(hey_ela_send(phraseArg(ctx), to.trim(), amount.trim(), auth))
        return if (o.has("error")) Result.failure(RuntimeException(o.getString("error"))) else Result.success(o.optString("txHash"))
    }
    /** Basic check for an Elastos mainchain 'E…' address (full check is in Rust on send). */
    fun isElaAddress(s: String): Boolean = s.trim().let { it.length in 33..34 && it.startsWith("E") }
    /** Registered EVM chains (esc, ethereum, …) from the Rust registry. */
    fun walletChains(): List<ChainInfo> = runCatching {
        val arr = JSONArray(hey_wallet_chains())
        (0 until arr.length()).map { val o = arr.getJSONObject(it); ChainInfo(o.optString("key"), o.optString("name"), o.optInt("chainId"), o.optString("symbol")) }
    }.getOrDefault(emptyList())
    /** Native balance on a given EVM chain (key = "esc"/"ethereum"/…). */
    fun walletBalance(ctx: android.content.Context, chain: String): WalletInfo? {
        val o = JSONObject(hey_wallet_balance(phraseArg(ctx), chain))
        if (o.has("error")) return null
        return WalletInfo(o.optString("address"), o.optString("balance"), o.optString("wei"), o.optString("symbol"))
    }
    /** Sign + broadcast a real transfer on `chain`. amount is a decimal string.
     *  `auth` = the spend grant from authorizeEvmSend — without it Rust refuses. */
    fun walletSend(ctx: android.content.Context, chain: String, to: String, amount: String, auth: String): Result<String> {
        val wei = toWeiHex(amount) ?: return Result.failure(IllegalArgumentException("Invalid amount"))
        val o = JSONObject(hey_wallet_send(phraseArg(ctx), chain, to.trim(), wei, auth))
        return if (o.has("error")) Result.failure(RuntimeException(o.getString("error")))
        else Result.success(o.optString("txHash"))
    }
    /** Decimal token amount → wei as a hex string (no 0x); EVM native = 18 decimals. */
    fun toWeiHex(amount: String): String? = toUnitsHex(amount, 18)
    /** Decimal amount → smallest-units hex (no 0x) for `decimals` places. null if unclean. */
    fun toUnitsHex(amount: String, decimals: Int): String? = runCatching {
        val v = java.math.BigDecimal(amount.trim()).movePointRight(decimals).toBigIntegerExact()
        if (v.signum() < 0) null else v.toString(16)
    }.getOrNull()

    /** Native + curated ERC-20 balances on `chain` (scam-safe: curated list only),
     *  with locally-hidden tokens filtered out unless includeHidden. */
    fun balances(ctx: android.content.Context, chain: String, includeHidden: Boolean = false): List<TokenBal> {
        val o = JSONObject(hey_wallet_balances(phraseArg(ctx), chain))
        val arr = o.optJSONArray("tokens") ?: return emptyList()
        val hidden = hiddenTokens(ctx)
        return (0 until arr.length()).map {
            val t = arr.getJSONObject(it)
            TokenBal(t.optString("symbol"), t.optString("name"), t.optString("contract"), t.optInt("decimals"),
                t.optBoolean("native"), t.optString("balance"), t.optString("raw"))
        }.filter { includeHidden || it.native || "$chain:${it.contract}" !in hidden }
    }
    /** Send an ERC-20 token. amount is a decimal string in token units.
     *  `auth` = the spend grant from authorizeTokenSend — without it Rust refuses. */
    fun tokenSend(ctx: android.content.Context, chain: String, contract: String, to: String, amount: String, decimals: Int, auth: String): Result<String> {
        val units = toUnitsHex(amount, decimals) ?: return Result.failure(IllegalArgumentException("Invalid amount"))
        val o = JSONObject(hey_wallet_token_send(phraseArg(ctx), chain, contract, to.trim(), units, auth))
        return if (o.has("error")) Result.failure(RuntimeException(o.getString("error"))) else Result.success(o.optString("txHash"))
    }

    // ── hidden tokens (scam protection) — local pref set of "chain:contract" ──
    private fun hiddenTokens(ctx: android.content.Context): Set<String> =
        ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE).getStringSet("hidden_tokens", emptySet()) ?: emptySet()
    fun setTokenHidden(ctx: android.content.Context, chain: String, contract: String, hidden: Boolean) {
        val p = ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE)
        val s = HashSet(p.getStringSet("hidden_tokens", emptySet()) ?: emptySet())
        if (hidden) s.add("$chain:$contract") else s.remove("$chain:$contract")
        p.edit().putStringSet("hidden_tokens", s).apply()
    }
    fun hiddenCount(ctx: android.content.Context, chain: String): Int = hiddenTokens(ctx).count { it.startsWith("$chain:") }
    fun isTokenHidden(ctx: android.content.Context, chain: String, contract: String): Boolean = "$chain:$contract" in hiddenTokens(ctx)

    // ── NFTs (collectibles) ──────────────────────────────────────────────────
    /** Every NFT the wallet holds on `chain`. Default = the open-source Blockscout
     *  index (mode="index", complete); when the index is set to "off" the result is
     *  curated + user-tracked collections only (mode="tracked" → label it "tracked
     *  collections", not "all your NFTs"). Hidden tiles are filtered unless asked. */
    fun nfts(ctx: android.content.Context, chain: String, includeHidden: Boolean = false): NftList {
        val added = JSONArray(pinnedNftCollections(ctx, chain).toList())
        val o = JSONObject(hey_wallet_nfts(phraseArg(ctx), chain, added.toString()))
        if (o.has("error")) return NftList("tracked", emptyList())
        val mode = o.optString("mode", "tracked")
        val colls = o.optJSONArray("collections") ?: JSONArray()
        val hidden = hiddenNfts(ctx)
        val out = ArrayList<HeyNft>()
        for (i in 0 until colls.length()) {
            val c = colls.getJSONObject(i)
            val contract = c.optString("contract")
            val collName = c.optString("name")
            val kind = c.optString("kind", "721")
            val insts = c.optJSONArray("instances") ?: continue
            for (j in 0 until insts.length()) {
                val it = insts.getJSONObject(j)
                val id = it.optString("id")
                if (!includeHidden && "$chain:$contract:$id" in hidden) continue
                out.add(HeyNft(collName, contract, id, kind, it.optString("name"), it.optString("image"), it.optString("amount", "1")))
            }
        }
        return NftList(mode, out)
    }
    /** Look up + verify a manually-added NFT (contract + decimal token_id). */
    fun nftLookup(ctx: android.content.Context, chain: String, contract: String, tokenId: String): JSONObject =
        runCatching { JSONObject(hey_wallet_nft_lookup(phraseArg(ctx), chain, contract.trim(), tokenId.trim())) }.getOrDefault(JSONObject())
    /** MONEY: send an ERC-721 NFT. tokenId is DECIMAL. `auth` = grant from authorizeNftSend721. */
    fun nftSend721(ctx: android.content.Context, chain: String, contract: String, to: String, tokenId: String, auth: String): Result<String> {
        val o = JSONObject(hey_wallet_nft_send_721(phraseArg(ctx), chain, contract.trim(), to.trim(), tokenId.trim(), auth))
        return if (o.has("error")) Result.failure(RuntimeException(o.getString("error"))) else Result.success(o.optString("txHash"))
    }
    /** MONEY: send `qty` of an ERC-1155 token id. tokenId + qty DECIMAL. `auth` = grant from authorizeNftSend1155. */
    fun nftSend1155(ctx: android.content.Context, chain: String, contract: String, to: String, tokenId: String, qty: String, auth: String): Result<String> {
        val o = JSONObject(hey_wallet_nft_send_1155(phraseArg(ctx), chain, contract.trim(), to.trim(), tokenId.trim(), qty.trim(), auth))
        return if (o.has("error")) Result.failure(RuntimeException(o.getString("error"))) else Result.success(o.optString("txHash"))
    }

    // ── hidden NFTs (scam-airdrop defense) — pref set of "chain:contract:id" ──
    private fun hiddenNfts(ctx: android.content.Context): Set<String> = prefSet(ctx, "hidden_nfts")
    fun isNftHidden(ctx: android.content.Context, chain: String, contract: String, id: String): Boolean = "$chain:$contract:$id" in hiddenNfts(ctx)
    fun setNftHidden(ctx: android.content.Context, chain: String, contract: String, id: String, hidden: Boolean) =
        prefToggle(ctx, "hidden_nfts", "$chain:$contract:$id", hidden)
    fun hiddenNftCount(ctx: android.content.Context, chain: String): Int = hiddenNfts(ctx).count { it.startsWith("$chain:") }

    // ── manually tracked NFT collections (for the indexer-off / trustless mode) ──
    private fun pinnedNftCollections(ctx: android.content.Context, chain: String): Set<String> =
        prefSet(ctx, "pinned_nft_collections").filter { it.startsWith("$chain:") }.map { it.removePrefix("$chain:") }.toSet()
    fun addPinnedNftCollection(ctx: android.content.Context, chain: String, contract: String) =
        prefToggle(ctx, "pinned_nft_collections", "$chain:${contract.trim()}", true)
    fun removePinnedNftCollection(ctx: android.content.Context, chain: String, contract: String) =
        prefToggle(ctx, "pinned_nft_collections", "$chain:${contract.trim()}", false)

    // ── wallet settings + local transaction history ──────────────────────────
    private fun walletPrefs(ctx: android.content.Context) = ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE)
    fun showTxHistory(ctx: android.content.Context): Boolean = walletPrefs(ctx).getBoolean("show_tx_history", true)
    fun setShowTxHistory(ctx: android.content.Context, v: Boolean) = walletPrefs(ctx).edit().putBoolean("show_tx_history", v).apply()
    /** BEAM node mode. Three values:
     *   - "quicksync" (DEFAULT): the wallet's FlyClient syncs from a public BEAM node
     *     (BeamApi.DEFAULT_NODE) — one outbound sync socket, sealed in libbeam, same category as the
     *     EVM/ELA wallet RPC. NO on-device node, NO 127.0.0.1 loopback.
     *   - "mobilenode": opt-in max-privacy. Runs a private beam::Node in-process (loopback-only
     *     listener; the wallet talks to it over 127.0.0.1). The node syncs the chain directly from
     *     mainnet peers, so no public node correlates your IP with your coin requests. Heavy: ~GBs,
     *     first sync can take a while, more battery/data — Wi-Fi + charger recommended.
     *   - "ownnode": FlyClient pointed at a BEAM node the user hosts elsewhere (host:port). */
    fun beamNodeMode(ctx: android.content.Context): String = walletPrefs(ctx).getString("beam_node_mode", "quicksync") ?: "quicksync"
    fun setBeamNodeMode(ctx: android.content.Context, m: String) = walletPrefs(ctx).edit().putString("beam_node_mode", m).apply()

    /** Record a tx Hey itself sent (so we have a real history without an indexer).
     *  Received history needs a chain explorer API — added later. Capped at 200.
     *  M1: SEALED at rest (SealedPrefs / hardware-wrapped AES-GCM) instead of the
     *  plaintext "hey" SharedPreferences, which re-exposed the financial+social trail
     *  the Rust audit log deliberately seals. For tips, `to` should be the recipient
     *  DID (not a display name) — see the tip call site. */
    private const val TX_HISTORY_KEY = "tx_history"
    fun recordTx(ctx: android.content.Context, chain: String, symbol: String, to: String, amount: String, hash: String, kind: String = "sent") {
        runCatching {
            val arr = JSONArray(SealedPrefs.get(ctx, TX_HISTORY_KEY, "[]"))
            val o = JSONObject().put("chain", chain).put("symbol", symbol).put("to", to).put("amount", amount)
                .put("hash", hash).put("kind", kind).put("ts", System.currentTimeMillis())
            val out = JSONArray().put(o)
            for (i in 0 until minOf(arr.length(), 199)) out.put(arr.getJSONObject(i))
            SealedPrefs.put(ctx, TX_HISTORY_KEY, out.toString())
        }
    }
    /** Local tx history, newest first (sealed at rest). Migrates a legacy plaintext
     *  "hey".tx_history into the seal once, then wipes the plaintext copy. */
    fun txHistory(ctx: android.content.Context): List<TxRecord> = runCatching {
        // One-time migration of any pre-M1 plaintext history → sealed, then purge it.
        val legacy = walletPrefs(ctx).getString(TX_HISTORY_KEY, null)
        if (!legacy.isNullOrBlank() && legacy != "[]") {
            if (SealedPrefs.get(ctx, TX_HISTORY_KEY, "").isBlank()) SealedPrefs.put(ctx, TX_HISTORY_KEY, legacy)
            walletPrefs(ctx).edit().remove(TX_HISTORY_KEY).apply()
        }
        val arr = JSONArray(SealedPrefs.get(ctx, TX_HISTORY_KEY, "[]"))
        (0 until arr.length()).map { val o = arr.getJSONObject(it); TxRecord(o.optString("chain"), o.optString("symbol"), o.optString("to"), o.optString("amount"), o.optString("hash"), o.optString("kind"), o.optLong("ts")) }
    }.getOrDefault(emptyList())
    /** Full EIP-55 checksum + zero-address validation (in Rust). Returns the
     *  canonical checksummed address on success, or an error message — so a typo'd
     *  or burn address can't be sent to. */
    fun checkAddress(addr: String): Result<String> {
        val o = JSONObject(hey_wallet_check_address(addr.trim()))
        return if (o.optBoolean("ok")) Result.success(o.optString("address"))
        else Result.failure(IllegalArgumentException(o.optString("error", "Invalid address")))
    }
    /** Broadcast tx confirmation state on `chain`: "pending" | "success" | "failed". */
    fun txStatus(chain: String, hash: String): String =
        runCatching { JSONObject(hey_wallet_tx_status(chain, hash)).optString("status", "pending") }.getOrDefault("pending")

    // ── tipping ────────────────────────────────────────────────────────────
    /** Publish my receive addresses (EVM 0x for every EVM chain + ELA E…) in my
     *  signed profile, so followers can tip me by identity — no address sharing. */
    fun publishTipAddresses(ctx: android.content.Context): Boolean {
        val evm = walletAddress(ctx) ?: return false
        val o = JSONObject()
        walletChains().forEach { o.put(it.key, evm) }   // esc, ethereum, … all share the 0x
        elaAddress(ctx)?.let { o.put("ela", it) }
        beamAddress(ctx)?.let { o.put("beam", it) }      // BEAM static public_offline donation token
        return runCatching { !JSONObject(hey_set_tip_addresses(o.toString())).has("error") }.getOrDefault(false)
    }
    /** A peer's published receive addresses: chainKey -> address. Empty if none. */
    fun resolveTip(did: String): Map<String, String> = parseAddrs(runCatching { hey_resolve_tip(did) }.getOrDefault(""))

    /** Like resolveTip but ALSO exchanges addresses over the DM channel first, so tipping
     *  a chat contact resolves even without following them. Call off the main thread. */
    fun refreshContact(did: String): Map<String, String> = parseAddrs(runCatching { hey_refresh_contact(did) }.getOrDefault(""))

    private fun parseAddrs(raw: String): Map<String, String> {
        if (!raw.trim().startsWith("{")) return emptyMap()
        val o = JSONObject(raw)
        val m = LinkedHashMap<String, String>()
        o.keys().forEach { k -> o.optString(k).takeIf { it.isNotBlank() }?.let { m[k] = it } }
        return m
    }

    /** After an on-chain tip confirms, notify the recipient over the carrier so they
     *  get a "sent you a tip" notification with the app closed. Fire-and-forget;
     *  failures are harmless (the transfer already landed on-chain). Call off-main. */
    fun notifyTip(toDid: String, symbol: String, amount: String, txHash: String) {
        runCatching { hey_notify_tip(toDid, symbol, amount, txHash) }
    }
}

data class WalletInfo(val address: String, val balance: String, val wei: String, val symbol: String)
data class ChainInfo(val key: String, val name: String, val chainId: Int, val symbol: String)
data class TokenBal(val symbol: String, val name: String, val contract: String, val decimals: Int, val native: Boolean, val balance: String, val raw: String)
data class TxRecord(val chain: String, val symbol: String, val to: String, val amount: String, val hash: String, val kind: String, val ts: Long)
/** One collectible the wallet holds. `tokenId` is the DECIMAL uint256; `type` =
 *  "721"|"1155"; `amount` = owned count (1 for 721, N for 1155). */
data class HeyNft(val collection: String, val contract: String, val tokenId: String, val type: String, val name: String, val image: String, val amount: String) {
    val is1155 get() = type == "1155"
}
/** `mode` = "index" (complete, from the explorer) | "tracked" (curated/eth_call only). */
data class NftList(val mode: String, val items: List<HeyNft>)

data class Media(val cid: String, val mime: String, val type: String, val name: String)
data class Follow(val did: String, val ticket: String)
data class Chat(val id: String, val name: String, val preview: String, val ts: Long, val unread: Int, val isGroup: Boolean, val avatar: String = "")

/** One inbound 1:1 call-control signal. `type` = offer | accept | decline | end. */
data class CallSignal(val from: String, val type: String, val callId: String, val payload: JSONObject)
data class Attachment(val name: String, val mime: String, val size: Long, val raw: String) {
    val isImage get() = mime.startsWith("image/")
    val isVideo get() = mime.startsWith("video/")
    /** True for torrent-style streamed attachments → fetch via fetchAttachmentToPath (to disk). */
    val isStreamed get() = runCatching { JSONObject(raw).optBoolean("streamed") }.getOrDefault(false)
}
data class Msg(
    val id: String, val text: String, val ts: Long, val mine: Boolean, val sender: String,
    val attachments: List<Attachment> = emptyList(),
)
data class MsgReaction(val messageId: String, val emoji: String, val sender: String)

data class Profile(val did: String, val nickname: String, val bio: String, val avatar: String) {
    companion object {
        fun from(o: JSONObject) = Profile(
            o.optString("did"), o.optString("nickname"), o.optString("bio"), o.optString("avatar")
        )
    }
}

data class Reactions(val likeCount: Int, val liked: Boolean, val total: Int) {
    companion object {
        fun from(o: JSONObject): Reactions {
            val counts = o.optJSONObject("counts")
            val like = counts?.optInt(HeyApi.LIKE, 0) ?: 0
            return Reactions(like, o.optString("mine") == HeyApi.LIKE, o.optInt("total"))
        }
    }
}

data class Comment(
    val id: String, val author: String, val authorName: String,
    val text: String, val ts: Long, val parent: String,
) {
    companion object {
        fun from(o: JSONObject) = Comment(
            o.optString("id"), o.optString("author"), o.optString("author_name"),
            o.optString("text"), o.optLong("ts"), o.optString("parent")
        )
    }
}

data class Post(
    val id: String,
    val author: String,
    val authorName: String,
    val authorAvatar: String,
    val caption: String,
    val ts: Long,
    val media: List<Media>,
) {
    companion object {
        fun from(o: JSONObject): Post {
            val m = o.optJSONArray("media") ?: JSONArray()
            val media = (0 until m.length()).map {
                val t = m.getJSONObject(it)
                Media(t.optString("cid"), t.optString("mime"), t.optString("type", "photo"), t.optString("name"))
            }
            return Post(
                o.optString("id"), o.optString("author"), o.optString("author_name"),
                o.optString("author_avatar"), o.optString("caption"), o.optLong("ts"), media
            )
        }
    }
}
