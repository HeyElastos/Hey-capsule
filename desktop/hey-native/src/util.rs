//! Small host-side utilities: image downscale/encode for uploads (keeps posts +
//! avatars under the runtime's content body limit) and extension→mime guessing.

/// Prepare a picked file for upload. Images are downscaled to ≤1600px on the long
/// edge and re-encoded as JPEG q82 (so a multi-MB photo stays well under the
/// runtime's ~2MB content body limit). Non-images (video, etc.) pass through raw
/// with an extension-derived mime so `upload_media` tags them correctly.
pub fn process_media(bytes: Vec<u8>, name: &str) -> (Vec<u8>, String) {
    if let Ok(img) = image::load_from_memory(&bytes) {
        let img = if img.width() > 1600 || img.height() > 1600 {
            img.resize(1600, 1600, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };
        let mut out = std::io::Cursor::new(Vec::new());
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82);
        if enc.encode_image(&img).is_ok() {
            return (out.into_inner(), "image/jpeg".to_string());
        }
        // fall through to raw if encode failed
    }
    (bytes, mime_from_name(name))
}

/// Downscale + JPEG-encode an avatar to a small square-ish image.
pub fn process_avatar(bytes: Vec<u8>) -> (Vec<u8>, String) {
    if let Ok(img) = image::load_from_memory(&bytes) {
        let img = img.resize(512, 512, image::imageops::FilterType::Lanczos3);
        let mut out = std::io::Cursor::new(Vec::new());
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
        if enc.encode_image(&img).is_ok() {
            return (out.into_inner(), "image/jpeg".to_string());
        }
    }
    (bytes, "image/jpeg".to_string())
}

pub fn mime_from_name(name: &str) -> String {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "m4v" => "video/x-m4v",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// Human-readable byte size for attachment rows.
pub fn human_size(n: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", U[i])
    }
}
