// Client-side image compression.
//
// Resize-to-2048 + re-encode to WebP entirely in the browser, BEFORE the upload
// reaches the runtime's provider body limit (axum's 2 MB default on
// /api/provider/*). The Hey pack ships no hey-transcoder provider, so without
// this every photo uploads RAW (a 2.6 MB phone photo is ~3.5 MB once base64'd
// in the JSON publish body) and the runtime 413s it. Doing the shrink here
// keeps the request small and needs no runtime patch / full upgrade.
//
// WebP (not AVIF) because browser <canvas> can encode WebP via toDataURL but
// not AVIF. WebP q0.82 at ≤2048px brings a typical phone photo to a few hundred
// KB with no visible loss. Every failure path falls back to the original bytes,
// so this can only make an upload smaller — never block it.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

const MAX_DIM: f64 = 2048.0;
const WEBP_QUALITY: f64 = 0.82;
// Below this, re-encoding rarely helps (and a webp of an already-small jpeg can
// even grow it), so leave tiny images untouched.
const MIN_BYTES_TO_COMPRESS: usize = 64 * 1024;

/// Returns `(webp_bytes, "image/webp")` on success, or `None` to keep the
/// originals (non-image, animated gif, tiny, decode/encode failure, or the
/// result wasn't actually smaller).
pub async fn compress_image(bytes: &[u8], mime: &str) -> Option<(Vec<u8>, String)> {
    if !mime.starts_with("image/") || mime == "image/gif" {
        return None;
    }
    if bytes.len() < MIN_BYTES_TO_COMPRESS {
        return None;
    }
    let win = web_sys::window()?;
    let document = win.document()?;

    // Decode the bytes via createImageBitmap (it sniffs the format).
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::of1(&arr);
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).ok()?;
    let bitmap: web_sys::ImageBitmap =
        JsFuture::from(win.create_image_bitmap_with_blob(&blob).ok()?)
            .await
            .ok()?
            .dyn_into()
            .ok()?;

    let (sw, sh) = (bitmap.width() as f64, bitmap.height() as f64);
    if sw < 1.0 || sh < 1.0 {
        bitmap.close();
        return None;
    }
    // Scale so the longest side is ≤ MAX_DIM (never upscale).
    let scale = (MAX_DIM / sw).min(MAX_DIM / sh).min(1.0);
    let dw = (sw * scale).round().max(1.0);
    let dh = (sh * scale).round().max(1.0);

    let canvas: web_sys::HtmlCanvasElement =
        document.create_element("canvas").ok()?.dyn_into().ok()?;
    canvas.set_width(dw as u32);
    canvas.set_height(dh as u32);
    let ctx: web_sys::CanvasRenderingContext2d =
        canvas.get_context("2d").ok()??.dyn_into().ok()?;
    ctx.draw_image_with_image_bitmap_and_dw_and_dh(&bitmap, 0.0, 0.0, dw, dh)
        .ok()?;
    bitmap.close();

    let data_url = canvas
        .to_data_url_with_type_and_encoder_options("image/webp", &JsValue::from_f64(WEBP_QUALITY))
        .ok()?;
    // Some browsers ignore the requested type and hand back image/png; only
    // accept a real webp result so we don't claim a wrong mime.
    if !data_url.starts_with("data:image/webp") {
        return None;
    }
    let comma = data_url.find(',')?;
    let out = B64.decode(data_url[comma + 1..].as_bytes()).ok()?;
    if out.is_empty() || out.len() >= bytes.len() {
        return None; // didn't actually shrink it
    }
    Some((out, "image/webp".to_string()))
}
