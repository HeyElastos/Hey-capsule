package os.elastos.hey.social

import org.json.JSONObject
import java.math.BigDecimal

/**
 * Kotlin bridge to Hey's BEAM shim (libbeam.so — built by mobile/beam/build-beam.sh, separate from
 * libhey_mobile_runtime.so). BEAM is Mimblewimble/C++; the .so wraps BEAM wallet-core.
 *
 * Stateless, same contract as the Rust wallet bridge: the mnemonic (Hey's single recovery phrase,
 * which ALSO controls the BEAM wallet — verified BIP39 parity) is passed per call, used, dropped.
 * Call everything OFF the main thread (Dispatchers.IO). `available` is false until libbeam.so ships
 * in the APK, so the rest of the app never crashes if BEAM isn't built into this build.
 *
 * Money-safety: beam_send stays disabled in the shim until the gate in docs/HEY_BEAM_INTEGRATION.md
 * passes (testnet public_offline tx + a sub-cent mainnet tx). 1 BEAM = 100_000_000 groth.
 */
object BeamApi {
    /** True once libbeam.so loaded — i.e. this APK was built with BEAM (Phase 1+). */
    val available: Boolean = runCatching { System.loadLibrary("beam"); true }.getOrDefault(false)

    private const val GROTH_PER_BEAM = 100_000_000L
    // CURRENT mainnet seed (matches BEAM's getDefaultPeers(); W1). Overridable via prefs later.
    const val DEFAULT_NODE = "eu-nodes.mainnet.beam.mw:8100"
    /** Asset ids (same wallet/address; the asset is a tx-level field — like ETH vs an ERC-20). */
    const val ASSET_BEAM = 0
    const val ASSET_BEAMX = 7
    /** Money-safety: first real sends are hard-capped sub-cent until the user lifts it after a test.
     *  THE REAL CAP IS ENFORCED IN RUST (guard.rs check_beam_cap, called from hey_beam_send) — this
     *  SharedPref + these numbers are UX-only (greying the Send button, showing the limit text). A
     *  flipped SharedPref can NOT lift the actual cap; only hey_beam_lift_cap (behind a fresh auth) can. */
    const val SEND_CAP_GROTH = 1_000_000L            // 0.01 BEAM (must match guard.rs BEAM_SEND_CAP_GROTH)
    val SEND_CAP_BEAM: String get() = formatBeam(SEND_CAP_GROTH)
    fun capLifted(ctx: android.content.Context): Boolean =
        ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE).getBoolean("beam_cap_lifted", false)
    /** UX mirror only. Also flips the Rust in-process cap (the REAL gate): on → lift, off → reset. */
    fun setCapLifted(ctx: android.content.Context, v: Boolean) {
        ctx.getSharedPreferences("hey", android.content.Context.MODE_PRIVATE).edit().putBoolean("beam_cap_lifted", v).apply()
        runCatching { if (v) HeyApi.hey_beam_lift_cap() else HeyApi.hey_beam_reset_cap() }
    }

    // ── native (os.elastos.hey.social.BeamApi) — `dir` is BEAM's persistent WalletDB dir ──────
    // H1/H5: the mnemonic-taking C++ ops below are NO LONGER called from Kotlin — the Rust runtime
    // invokes them via JNI (hey_beam_*), passing the IN-PROCESS phrase so the mnemonic never crosses
    // JNI from Kotlin. They stay declared as registered native methods (proguard -keep BeamApi) so the
    // JVM resolves the symbols when Rust calls os/elastos/hey/social/BeamApi.<name>. Methods with NO
    // mnemonic arg (validate_token, sync_progress, node_stop/status) are still called directly.
    @JvmStatic @Suppress("unused") private external fun beam_address(mnemonic: String, dir: String): String
    @JvmStatic @Suppress("unused") private external fun beam_balance(mnemonic: String, dir: String, nodeUri: String): String
    private external fun beam_validate_token(token: String): String
    // H-1/H1-1: arm the in-process BEAM signer for exactly (token, amountGroth, assetId, nonce).
    // Called by the Rust hey_beam_send via JNI IMMEDIATELY before beam_send, AFTER the
    // spend grant is redeemed + the cap enforced in guard.rs. The C++ send() refuses
    // unless armed for this exact transfer AND nonce, and consumes the arm single-use keyed
    // on the FRESH per-send nonce (so two identical legitimate sends can't share an arm).
    // Declared here (not called from Kotlin) only so the JVM resolves the registered symbol
    // when Rust calls os/elastos/hey/social/BeamApi.beam_arm_send.
    @JvmStatic @Suppress("unused") private external fun beam_arm_send(token: String, amountGroth: Long, assetId: Int, nonce: String)
    @JvmStatic @Suppress("unused") private external fun beam_send(mnemonic: String, dir: String, nodeUri: String, token: String, amountGroth: Long, feeGroth: Long, assetId: Int, nonce: String): String
    @JvmStatic @Suppress("unused") private external fun beam_tx_status(mnemonic: String, dir: String, nodeUri: String, txid: String): String
    @JvmStatic @Suppress("unused") private external fun beam_scan(mnemonic: String, dir: String, nodeUri: String): String
    private external fun beam_sync_progress(): String
    // ── on-device mainnet node (loopback) ─────────────────────────────────────
    @JvmStatic @Suppress("unused") private external fun beam_node_start(mnemonic: String, dir: String): String
    private external fun beam_node_stop(): String
    private external fun beam_node_status(): String
    @JvmStatic @Suppress("unused") private external fun beam_scan_local(mnemonic: String, dir: String, waitMs: Int): String

    /** BEAM's private WalletDB dir inside the app's files (created on demand). */
    fun beamDir(ctx: android.content.Context): String =
        java.io.File(ctx.filesDir, "beam").apply { mkdirs() }.absolutePath

    // ── helpers (return Result; never throw to the UI) ────────────────────────
    // H5: every BEAM op resolves the mnemonic IN-PROCESS via the Rust runtime
    // (hey_beam_*), so the phrase no longer crosses JNI from Kotlin for any path.

    /** The static public_offline donation token to publish for tipping (LOCAL — no node). */
    fun address(dir: String): Result<String> = guard {
        val o = JSONObject(HeyApi.hey_beam_address(dir))
        o.optString("token").takeIf { it.isNotBlank() } ?: throw IllegalStateException(o.optString("error", "no token"))
    }

    /** BEAM + BEAMX (asset 7) balances (decimal-BEAM strings) from the last sync. No network. */
    fun balance(dir: String): Result<BeamBalance> = guard {
        val o = JSONObject(HeyApi.hey_beam_balance(dir, ""))
        if (o.has("error")) throw IllegalStateException(o.getString("error"))
        val b = o.optJSONObject("beam") ?: JSONObject()
        val x = o.optJSONObject("beamx") ?: JSONObject()
        BeamBalance(
            formatBeam(b.optString("available", "0").toLongOrNull() ?: 0L),
            formatBeam(b.optString("maturing", "0").toLongOrNull() ?: 0L),
            formatBeam(x.optString("available", "0").toLongOrNull() ?: 0L),
        )
    }

    fun validToken(token: String): Boolean =
        available && runCatching { JSONObject(beam_validate_token(token)).optBoolean("valid", false) }.getOrDefault(false)

    /**
     * Build + broadcast a payment UNDER THE GUARD (H1). `amount` is decimal BEAM; `assetId` 0 = BEAM,
     * 7 = BEAMX. `auth` = the one-shot spend grant the user confirmed (kind="beam:<asset>", to=token,
     * amount=decimal BEAM). The mnemonic is resolved IN-PROCESS by Rust (never passed here); the cap +
     * grant are enforced in Rust (guard.rs). The UX checks below just give a fast local error. Returns
     * {txid, status}.
     */
    fun send(
        dir: String, token: String, amount: String, assetId: Int, auth: String,
        nodeUri: String = DEFAULT_NODE, feeGroth: Long = 100_000L,
    ): Result<BeamSendResult> = guard {
        val groth = toGroth(amount) ?: throw IllegalArgumentException("Enter a valid amount")
        if (groth <= 0L) throw IllegalArgumentException("Amount must be greater than 0")
        if (token.isBlank()) throw IllegalArgumentException("Enter a recipient address")
        if (auth.isBlank()) throw IllegalStateException("Confirm the transfer first")
        // Rust gates everything (redeem_spend + check_beam_cap) then invokes libbeam with the
        // in-process phrase — the mnemonic no longer crosses JNI from Kotlin.
        val o = JSONObject(HeyApi.hey_beam_send(token, amount.trim(), groth, feeGroth, assetId, dir, nodeUri, auth))
        val txid = o.optString("txid").takeIf { it.isNotBlank() }
            ?: throw IllegalStateException(o.optString("error", "send failed"))
        BeamSendResult(txid, o.optString("status", "pending"))
    }

    fun txStatus(dir: String, txid: String, nodeUri: String = DEFAULT_NODE): String =
        runCatching { JSONObject(HeyApi.hey_beam_tx_status(dir, nodeUri, txid)).optString("status", "unknown") }.getOrDefault("unknown")

    /** Connect + scan so received tips become visible. Returns the sync outcome (synced / still-syncing
     *  / the REAL error) so the UI shows progress or the actual failure — never a generic "couldn't sync". */
    fun scan(dir: String, nodeUri: String = DEFAULT_NODE): BeamScanResult {
        if (!available) return BeamScanResult(false, false, 0L, "BEAM not in this build")
        return runCatching {
            val o = JSONObject(HeyApi.hey_beam_scan(dir, nodeUri))
            if (o.has("error")) BeamScanResult(false, false, 0L, o.optString("error"))
            else BeamScanResult(o.optBoolean("ok", false), o.optBoolean("synced", false), o.optLong("height", 0L), null)
        }.getOrElse { BeamScanResult(false, false, 0L, it.message ?: "scan failed") }
    }

    // ── on-device node (loopback) — opt-in "Mobile node" mode ────────────────
    /** Start the in-process mainnet node. B3 FIX: this NO LONGER hard-fails on a pre-flight reachability
     *  probe — the node owns a resilient retry loop over all seeds, so it ALWAYS starts (returns true)
     *  barring a real init error (bad args / db open / kdf). Reachability is a NON-FATAL hint surfaced via
     *  nodeStatus().peersReachable. Call OFF the main thread. Idempotent in the shim. */
    fun nodeStart(dir: String): Boolean =
        available && runCatching { !JSONObject(HeyApi.hey_beam_node_start(dir)).has("error") }.getOrDefault(false)
    /** Start the node; returns "" on success/already-running, else the REAL error string from the
     *  shim (e.g. "wallet locked …", "beam: no master kdf", "beam: wallet db open failed") so the
     *  UI can show the actual cause instead of a generic "failed to start". Call OFF the main thread. */
    fun nodeStartError(dir: String): String =
        if (!available) "BEAM not in this build"
        else runCatching {
            val j = JSONObject(HeyApi.hey_beam_node_start(dir))
            if (j.has("error")) j.optString("error") else ""
        }.getOrElse { it.message ?: "node start failed" }
    /** Stop + tear down the node. The native dtor JOINS the node thread — call OFF the main thread (W4). */
    fun nodeStop() { if (available) runCatching { beam_node_stop() } }
    fun nodeStatus(): BeamNodeStatus =
        if (!available) BeamNodeStatus(false, false, 0L, 0L, false)
        else runCatching {
            val o = JSONObject(beam_node_status())
            BeamNodeStatus(o.optBoolean("running"), o.optBoolean("synced"), o.optLong("done"), o.optLong("total"),
                           o.optBoolean("peers_reachable", false))
        }.getOrDefault(BeamNodeStatus(false, false, 0L, 0L, false))
    /** Scan against the LOCAL node, gated on node-synced. While the node is still syncing (can take
     *  HOURS on first mainnet sync), returns nodeSyncing=true (NOT an error, B1) so the UI keeps
     *  polling nodeStatus() instead of showing a failure. Call OFF the main thread. */
    fun scanLocal(dir: String, waitMs: Int = 60_000): BeamScanResult {
        if (!available) return BeamScanResult(false, false, 0L, "BEAM not in this build")
        return runCatching {
            val o = JSONObject(HeyApi.hey_beam_scan_local(dir, waitMs))
            when {
                o.has("error")        -> BeamScanResult(false, false, 0L, o.optString("error"))
                o.optBoolean("node_syncing") -> BeamScanResult(false, false, 0L, null, nodeSyncing = true)
                else                  -> BeamScanResult(o.optBoolean("ok"), o.optBoolean("synced"), o.optLong("height", 0L), null)
            }
        }.getOrElse { BeamScanResult(false, false, 0L, it.message ?: "scan failed") }
    }

    /** Live sync snapshot (height + %) for the progress bar while a self-host scan runs. Poll ~1s. */
    fun syncProgress(): BeamSyncProgress =
        if (!available) BeamSyncProgress(0L, 0L, false, false, 0L)
        else runCatching {
            val o = JSONObject(beam_sync_progress())
            BeamSyncProgress(o.optLong("done", 0L), o.optLong("total", 0L), o.optBoolean("active", false), o.optBoolean("synced", false), o.optLong("height", 0L))
        }.getOrDefault(BeamSyncProgress(0L, 0L, false, false))

    // decimal BEAM <-> groth (8 decimals, no float)
    fun toGroth(amount: String): Long? = runCatching {
        BigDecimal(amount.trim()).movePointRight(8).toBigIntegerExact().let {
            if (it.signum() < 0) null else it.toLong()
        }
    }.getOrNull()
    fun formatBeam(groth: Long): String =
        BigDecimal(groth).movePointLeft(8).stripTrailingZeros().toPlainString()

    private inline fun <T> guard(block: () -> T): Result<T> =
        if (!available) Result.failure(IllegalStateException("BEAM not available in this build"))
        else runCatching { block() }
}

/** BEAM (asset 0) + BEAMX (asset 7) balances, decimal-BEAM strings. */
data class BeamBalance(val beam: String, val beamMaturing: String, val beamx: String)

/** Result of a send: the broadcast tx id (hex) + its current status (pending/confirmed/failed). */
data class BeamSendResult(val txid: String, val status: String)

/** Outcome of a sync. ok=ran without throwing; synced=reached the chain tip; height=node block height
 *  (self-host); error=the REAL native error when ok is false (shown verbatim so we can diagnose);
 *  nodeSyncing=the local node is still catching up (NOT an error — keep polling, B1). */
data class BeamScanResult(val ok: Boolean, val synced: Boolean, val height: Long, val error: String?, val nodeSyncing: Boolean = false)

/** On-device node status: running=node thread alive; synced=reached tip; done/total=block progress;
 *  peersReachable=NON-FATAL hint — true if any current mainnet seed answered on TCP at start (B3).
 *  There is no live accessible-peer count (BEAM doesn't expose it without source surgery — see
 *  node_status() in hey_beam_jni.cpp); done/total>0 is the connected/syncing proxy. */
data class BeamNodeStatus(val running: Boolean, val synced: Boolean, val done: Long, val total: Long,
                          val peersReachable: Boolean = false)

/** Live self-host sync snapshot for the progress UI. percent = done/total of the node's block sync. */
data class BeamSyncProgress(val done: Long, val total: Long, val active: Boolean, val synced: Boolean, val height: Long = 0L) {
    val percent: Int get() = if (total > 0L) ((done.coerceAtMost(total) * 100L) / total).toInt() else 0
}
