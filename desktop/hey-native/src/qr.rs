//! Friend-invite QR rendering. We rasterise qrcode's color matrix directly into
//! an egui ColorImage (no `image` dependency coupling, no extra rendering crate).
//!
//! A `hey:follow:` link carries the ML-KEM-768 public key (~2.5KB base64url), which
//! in QR byte-mode is right at the 2953-byte ceiling → a dense, barely-scannable
//! code. The Android app solves this by re-encoding the payload as `HEYF` + base32
//! (all chars in QR's *alphanumeric* set → ~22% headroom) for the QR ONLY, and
//! reversing it on scan. We replicate that byte-for-byte so a desktop-rendered QR
//! scans in the Android app (and vice-versa).

/// Render `text` as a black-on-white QR ColorImage, or None if it can't encode.
pub fn qr_image(text: &str) -> Option<egui::ColorImage> {
    let payload = to_qr_payload(text);
    // Lowest error correction (L) for the most capacity.
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::L).ok()?;
    let colors = code.to_colors();
    let modules = (colors.len() as f64).sqrt().round() as usize;
    if modules == 0 {
        return None;
    }
    // A friend-link QR is ~160 modules; below 3px/module phone cameras can't
    // resolve it off a monitor. Quiet zone = 4 modules per the QR spec.
    let scale = (560 / modules).clamp(3, 20);
    let quiet = 4 * scale;
    let dim = modules * scale + quiet * 2;
    let mut rgba = vec![255u8; dim * dim * 4]; // white background
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] == qrcode::Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = quiet + x * scale + dx;
                        let py = quiet + y * scale + dy;
                        let idx = (py * dim + px) * 4;
                        rgba[idx] = 0;
                        rgba[idx + 1] = 0;
                        rgba[idx + 2] = 0;
                        rgba[idx + 3] = 255;
                    }
                }
            }
        }
    }
    Some(egui::ColorImage::from_rgba_unmultiplied([dim, dim], &rgba))
}

/// Rasterise + upload a QR texture, memoised on the egui context. NEAREST
/// filtering is load-bearing: LINEAR blurs ~160-module friend-link codes into
/// unscannable gray mush. Display it at >= its native size — downscaling
/// re-destroys the modules.
pub fn qr_texture(ctx: &egui::Context, text: &str) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(("qr-tex", text));
    if let Some(t) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(t);
    }
    let img = qr_image(text)?;
    let tex = ctx.load_texture("qr", img, egui::TextureOptions::NEAREST);
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Mirror of the Android app's `QrLink.toQr`: a `hey:follow:<base64url>` link →
/// `"HEYF" + base32(raw payload bytes)` (compact, alphanumeric, scannable). Any
/// other text (a wallet address, a DID) passes through unchanged.
fn to_qr_payload(text: &str) -> String {
    if let Some(b64) = text.strip_prefix("hey:follow:") {
        if !b64.is_empty() {
            if let Some(raw) = b64url_decode(b64) {
                return format!("HEYF{}", b32_encode(&raw));
            }
        }
    }
    text.to_string()
}

/// Decode URL-safe (or standard) base64, padded or not.
fn b64url_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        })
    }
    let mut out = Vec::with_capacity(s.len() * 3 / 4 + 1);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &c in s.as_bytes() {
        if c == b'=' {
            continue;
        }
        let v = val(c)?;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

/// RFC4648 base32, uppercase, no padding — identical to the Android `b32enc`.
fn b32_encode(data: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut s = String::with_capacity(data.len() * 8 / 5 + 1);
    let mut buf: u32 = 0;
    let mut bits = 0u32;
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            s.push(A[((buf >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        s.push(A[((buf << (5 - bits)) & 0x1f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_match_android_format() {
        // "hey:follow:" + base64url("hello") = aGVsbG8  →  HEYF + base32("hello")
        let link = "hey:follow:aGVsbG8";
        let p = to_qr_payload(link);
        assert_eq!(p, "HEYFNBSWY3DP"); // base32("hello") = NBSWY3DP
    }

    #[test]
    fn passes_through_non_links() {
        assert_eq!(to_qr_payload("0xabc123"), "0xabc123");
    }

    /// Dump a realistic friend-link QR (ML-KEM-768-sized payload) to
    /// /tmp/hey_qr_test.png so an external scanner (zxing/zbar) can verify it
    /// decodes — run with `cargo test dump_realistic -- --ignored`.
    #[test]
    #[ignore]
    fn dump_realistic_friend_link_qr() {
        fn b64url(data: &[u8]) -> String {
            const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut s = String::new();
            for chunk in data.chunks(3) {
                let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
                let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
                for i in 0..=chunk.len() {
                    s.push(A[((n >> (18 - 6 * i)) & 63) as usize] as char);
                }
            }
            s
        }
        // Same shape my_friend_link() builds: did + relay ticket + X25519 + ML-KEM-768.
        let kem: Vec<u8> = (0..1184u32).map(|i| (i * 7 + 13) as u8).collect();
        let x: Vec<u8> = (0..32u8).collect();
        let ticket: Vec<u8> = (0..160u32).map(|i| (i * 3) as u8).collect();
        let json = format!(
            r#"{{"did":"did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK","ticket":"{}","x":"{}","k":"{}"}}"#,
            b64url(&ticket),
            b64url(&x),
            b64url(&kem),
        );
        let link = format!("hey:follow:{}", b64url(json.as_bytes()));
        let img = qr_image(&link).expect("encodes");
        let dim = img.size[0];
        let mut png = image::RgbaImage::new(dim as u32, dim as u32);
        for (i, px) in img.pixels.iter().enumerate() {
            let (x, y) = ((i % dim) as u32, (i / dim) as u32);
            png.put_pixel(x, y, image::Rgba(px.to_array()));
        }
        png.save("/tmp/hey_qr_test.png").expect("writes png");
        std::fs::write("/tmp/hey_qr_test_link.txt", &link).unwrap();
        println!("QR {}x{} px for a {}-char link", dim, dim, link.len());
    }
}
