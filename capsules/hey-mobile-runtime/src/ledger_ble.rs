//! Ledger BLE transport — the `0x05` APDU framing layer, owned in Rust so it has a
//! HARD per-exchange timeout and clean teardown. This is the differentiator: the
//! Elastos Essentials Android ELA-Ledger flow hangs forever because its
//! `BleTransport.exchange()` is timeout-less (docs/HEY_LEDGER_SUPPORT.md §0). Here,
//! `exchange()` ALWAYS returns within the timeout — on timeout it errors instead of
//! spinning, and the next op starts from a clean reassembly state.
//!
//! Kotlin is a dumb GATT pipe in BOTH directions (no protocol logic, no Rust→Kotlin
//! callback): it pushes inbound notify data via [`on_packet`], and PULLS the packets
//! we want written via [`take_outbound`] (its writer loop drains them to the write
//! characteristic). One source of truth for framing also serves a future USB-HID /
//! iOS transport unchanged.
//!
//! Framing (tag 0x05), big-endian:
//!   frame 0     : 05 | seq(2) | len(2) | payload(<= mtu-5)   (len = total APDU length)
//!   continuation: 05 | seq(2) |          payload(<= mtu-3)
//! MTU negotiation (tag 0x08): host writes `08 00 00 00 00`; device replies with the
//! agreed wire MTU at byte 5. We clamp it to the Android ATT MTU so a frame can never
//! exceed what a GATT write can carry (which would truncate and hang the device).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

const TAG_APDU: u8 = 0x05;
const TAG_MTU: u8 = 0x08;
/// Safe before ATT-MTU negotiation: BLE's default ATT MTU is 23 → 20 writable bytes.
const DEFAULT_MTU: usize = 20;
const MIN_MTU: usize = 8;
const MAX_MTU: usize = 255;

// ── a tiny blocking queue (Mutex<VecDeque> + Condvar) ───────────────────────
struct Chan {
    q: Mutex<VecDeque<Vec<u8>>>,
    cv: Condvar,
}
impl Chan {
    const fn new() -> Self {
        Chan { q: Mutex::new(VecDeque::new()), cv: Condvar::new() }
    }
    fn push(&self, v: Vec<u8>) {
        self.q.lock().unwrap().push_back(v);
        self.cv.notify_all();
    }
    fn pop_timeout(&self, d: Duration) -> Option<Vec<u8>> {
        let mut q = self.q.lock().unwrap();
        if let Some(v) = q.pop_front() {
            return Some(v);
        }
        let (mut q, _) = self.cv.wait_timeout_while(q, d, |q| q.is_empty()).unwrap();
        q.pop_front()
    }
    fn clear(&self) {
        self.q.lock().unwrap().clear();
    }
}

// ── reassembly of inbound 0x05 frames → a full APDU response ─────────────────
struct Reassembler {
    expected_seq: u16,
    declared_len: Option<usize>,
    buf: Vec<u8>,
}
impl Reassembler {
    const fn new() -> Self {
        Reassembler { expected_seq: 0, declared_len: None, buf: Vec::new() }
    }
    fn reset(&mut self) {
        self.expected_seq = 0;
        self.declared_len = None;
        self.buf.clear();
    }
    /// Feed one inbound 0x05 frame. Returns the complete APDU (data‖SW) once enough
    /// frames have arrived, or an error on a malformed/out-of-sequence frame (which
    /// also resets, so a fresh response can start clean).
    fn feed(&mut self, pkt: &[u8]) -> Result<Option<Vec<u8>>, String> {
        if pkt.len() < 3 || pkt[0] != TAG_APDU {
            return Err("ledger: bad frame tag/length".into());
        }
        let seq = u16::from_be_bytes([pkt[1], pkt[2]]);
        if seq != self.expected_seq {
            self.reset();
            return Err(format!("ledger: frame seq gap (got {seq}, want {})", self.expected_seq));
        }
        let payload: &[u8] = if seq == 0 {
            if pkt.len() < 5 {
                self.reset();
                return Err("ledger: short frame-0".into());
            }
            self.declared_len = Some(u16::from_be_bytes([pkt[3], pkt[4]]) as usize);
            self.buf.clear();
            &pkt[5..]
        } else {
            &pkt[3..]
        };
        self.buf.extend_from_slice(payload);
        self.expected_seq = self.expected_seq.wrapping_add(1);
        if let Some(len) = self.declared_len {
            if self.buf.len() >= len {
                self.buf.truncate(len);
                let apdu = std::mem::take(&mut self.buf);
                self.reset();
                return Ok(Some(apdu));
            }
        }
        Ok(None)
    }
}

/// Split an APDU into 0x05 frames sized to `mtu` (the agreed wire MTU).
fn frame_apdu(apdu: &[u8], mtu: usize) -> Vec<Vec<u8>> {
    let mtu = mtu.max(MIN_MTU);
    let mut frames = Vec::new();
    let mut seq: u16 = 0;
    let mut off = 0usize;
    loop {
        let header = if seq == 0 { 5 } else { 3 };
        let avail = mtu - header; // mtu >= MIN_MTU(8) > header, so > 0
        let take = (apdu.len() - off).min(avail);
        let mut f = Vec::with_capacity(header + take);
        f.push(TAG_APDU);
        f.extend_from_slice(&seq.to_be_bytes());
        if seq == 0 {
            f.extend_from_slice(&(apdu.len() as u16).to_be_bytes());
        }
        f.extend_from_slice(&apdu[off..off + take]);
        frames.push(f);
        off += take;
        seq = seq.wrapping_add(1);
        if off >= apdu.len() {
            break;
        }
    }
    frames
}

// ── shared transport state ──────────────────────────────────────────────────
static OUT: Chan = Chan::new(); // frames Rust wants written to the GATT write char
static RESP: Chan = Chan::new(); // completed APDU responses (+ an empty disconnect sentinel)
static NEG: Chan = Chan::new(); // 0x08 MTU-negotiation acknowledgements
static REASM: Mutex<Reassembler> = Mutex::new(Reassembler::new());
static MTU: AtomicUsize = AtomicUsize::new(DEFAULT_MTU); // agreed wire MTU (framing budget)
static ATT_WRITABLE: AtomicUsize = AtomicUsize::new(DEFAULT_MTU); // ATT MTU - 3 (write ceiling)
static CONNECTED: AtomicBool = AtomicBool::new(false);
static NEGOTIATED: AtomicBool = AtomicBool::new(false);

/// Kotlin → Rust: GATT link came up / went down. On down, wake any blocked
/// exchange with a disconnect sentinel and drop all transient state.
pub fn set_connected(connected: bool) {
    CONNECTED.store(connected, Ordering::Relaxed);
    if !connected {
        OUT.clear();
        NEG.clear();
        REASM.lock().unwrap().reset();
        NEGOTIATED.store(false, Ordering::Relaxed);
        MTU.store(DEFAULT_MTU, Ordering::Relaxed);
        RESP.push(Vec::new()); // empty => disconnected sentinel for a waiting exchange()
    }
}

/// Kotlin → Rust: the negotiated ATT MTU from `onMtuChanged`. We cap the 0x08 wire
/// MTU by (ATT MTU − 3) so a frame can never exceed a single GATT write.
pub fn set_att_mtu(att_mtu: usize) {
    let writable = att_mtu.saturating_sub(3).clamp(MIN_MTU, MAX_MTU);
    ATT_WRITABLE.store(writable, Ordering::Relaxed);
}

/// Kotlin → Rust: one inbound notify packet (0x05 APDU frame or 0x08 MTU reply).
pub fn on_packet(pkt: &[u8]) {
    if pkt.is_empty() {
        return;
    }
    match pkt[0] {
        TAG_MTU => {
            if pkt.len() >= 6 {
                let dev = pkt[5] as usize;
                let cap = ATT_WRITABLE.load(Ordering::Relaxed);
                MTU.store(dev.min(cap).clamp(MIN_MTU, MAX_MTU), Ordering::Relaxed);
            }
            NEG.push(vec![1]);
        }
        TAG_APDU => {
            let done = REASM.lock().unwrap().feed(pkt);
            match done {
                Ok(Some(apdu)) => RESP.push(apdu),
                Ok(None) => {}
                Err(e) => log::warn!("{e}"),
            }
        }
        other => log::warn!("ledger: unknown frame tag 0x{other:02x}"),
    }
}

/// Kotlin → Rust (writer loop): the next frame to write to the GATT write char, or
/// `None` if none arrived within `timeout` (the loop just calls again while connected).
pub fn take_outbound(timeout: Duration) -> Option<Vec<u8>> {
    OUT.pop_timeout(timeout)
}

/// Drop all transport state (between connections / on a hard reset).
pub fn reset() {
    set_connected(false);
}

/// Best-effort 0x08 MTU negotiation (idempotent). On timeout we keep DEFAULT_MTU,
/// which works for any ATT MTU — bigger frames are only a throughput win.
pub fn ensure_negotiated(timeout: Duration) {
    if NEGOTIATED.load(Ordering::Relaxed) {
        return;
    }
    NEG.clear();
    OUT.push(vec![TAG_MTU, 0, 0, 0, 0]);
    let ok = NEG.pop_timeout(timeout).is_some();
    NEGOTIATED.store(true, Ordering::Relaxed); // don't re-attempt every exchange
    if !ok {
        log::warn!("ledger: MTU negotiation timed out — using default {DEFAULT_MTU}");
    }
}

/// Send one APDU and await its response, ALWAYS within `timeout`. Strips and checks
/// SW1SW2 (0x9000 = ok). On timeout/disconnect, errors instead of hanging.
pub fn exchange(apdu: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
    if !CONNECTED.load(Ordering::Relaxed) {
        return Err("ledger not connected".into());
    }
    // Clear stale responses + reassembly so a previous timeout can't bleed in.
    RESP.clear();
    REASM.lock().unwrap().reset();
    for f in frame_apdu(apdu, MTU.load(Ordering::Relaxed)) {
        OUT.push(f);
    }
    let resp = RESP.pop_timeout(timeout).ok_or("ledger exchange timed out")?;
    if resp.is_empty() {
        return Err("ledger disconnected".into()); // the set_connected(false) sentinel
    }
    if resp.len() < 2 {
        return Err("ledger: response too short".into());
    }
    let sw = u16::from_be_bytes([resp[resp.len() - 2], resp[resp.len() - 1]]);
    if sw != 0x9000 {
        return Err(sw_message(sw));
    }
    Ok(resp[..resp.len() - 2].to_vec())
}

/// Human-readable message for a non-OK Ledger status word.
fn sw_message(sw: u16) -> String {
    match sw {
        0x6985 => "rejected on the Ledger".into(),
        0x6982 | 0x6804 => "open + unlock the Elastos app on your Ledger".into(),
        0x6d00 => "this Ledger app doesn't support that command".into(),
        0x6e00 => "wrong app open on the Ledger (open the Elastos app)".into(),
        0x6a86 | 0x6b00 => "ledger: wrong parameters".into(),
        0x6700 => "ledger: wrong length".into(),
        other => format!("ledger error 0x{other:04x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(apdu: &[u8], mtu: usize) -> Vec<u8> {
        let frames = frame_apdu(apdu, mtu);
        let mut r = Reassembler::new();
        let mut out = None;
        for f in &frames {
            if let Some(a) = r.feed(f).unwrap() {
                out = Some(a);
            }
        }
        out.expect("reassembled an APDU")
    }

    #[test]
    fn frame_reassemble_roundtrip() {
        for &mtu in &[20usize, 23, 100, 153, 255] {
            for len in [0usize, 1, 5, 19, 20, 50, 255, 256, 1000, 1020] {
                let apdu: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
                assert_eq!(roundtrip(&apdu, mtu), apdu, "mtu={mtu} len={len}");
            }
        }
    }

    #[test]
    fn frame_header_shapes() {
        let frames = frame_apdu(&[0xAA; 50], 20);
        // frame 0: tag, seq=0, len=50 hi/lo, then 15 payload bytes (mtu-5)
        assert_eq!(&frames[0][..5], &[0x05, 0x00, 0x00, 0x00, 50]);
        assert_eq!(frames[0].len(), 20);
        // continuation: tag, seq=1, then 17 payload bytes (mtu-3)
        assert_eq!(&frames[1][..3], &[0x05, 0x00, 0x01]);
    }

    #[test]
    fn reassembler_rejects_seq_gap() {
        let frames = frame_apdu(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 20);
        assert!(frames.len() >= 2);
        let mut r = Reassembler::new();
        r.feed(&frames[0]).unwrap();
        // skip frame 1, feed frame 2 (if present) → seq gap
        if frames.len() >= 3 {
            assert!(r.feed(&frames[2]).is_err());
        }
    }
}
