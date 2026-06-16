//! Tab views + shared widget helpers. Helpers take the specific App fields they
//! need (media/engine/ev_tx) rather than `&mut App`, so a view can hold an
//! immutable borrow of `app.state.*` while still loading textures and dispatching.

pub mod activity;
pub mod chat;
pub mod chat_sheets;
pub mod composer;
pub mod feed;
pub mod profile;
pub mod profile_sheets;
pub mod user_profile;
pub mod verse;
pub mod wallet;
pub mod welcome;

use std::sync::mpsc::Sender;

use egui::{Align2, Color32, FontId, Margin, RichText, Sense, Stroke};

use crate::engine::Engine;
use crate::media::MediaCache;
use crate::state::{AppState, UiEvent};
use crate::theme::{lerp, Theme, GOLD, GOLD_BRIGHT, NAVY};

// ── component library (the design system, used across views) ──────────────────

/// Flat soft push button: fill, radius 12, on-ink text, spring scale-on-press,
/// NO sheen. `primary_button` is the gold variant (keeps its name + signature).
pub fn push_button(
    ui: &mut egui::Ui,
    full_width: bool,
    text: &str,
    base: Color32,
    hover: Color32,
    ink: Color32,
) -> egui::Response {
    let h = 46.0;
    let w = if full_width {
        ui.available_width()
    } else {
        ui.painter()
            .layout_no_wrap(text.to_string(), FontId::new(15.0, crate::icons::semibold()), ink)
            .size()
            .x
            + 44.0
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h), Sense::click());
    let down = resp.is_pointer_button_down_on();
    let press = ui.ctx().animate_bool_with_time(resp.id, down, 0.07);
    let r = egui::Rect::from_center_size(rect.center(), rect.size() * (1.0 - 0.03 * press));
    let fill = if resp.hovered() && !down { hover } else { base };
    ui.painter()
        .rect_filled(r, 12.0, fill.gamma_multiply(1.0 - 0.06 * press));
    ui.painter().text(
        r.center(),
        Align2::CENTER_CENTER,
        text,
        FontId::new(15.0, crate::icons::semibold()),
        ink,
    );
    resp
}

/// The primary action button: flat gold capsule, on-gold ink, spring press.
pub fn primary_button(ui: &mut egui::Ui, full_width: bool, text: &str) -> egui::Response {
    push_button(ui, full_width, text, GOLD, GOLD_BRIGHT, NAVY)
}

/// Tinted gold secondary (Edit profile / Invite / copy chips).
pub fn secondary_button(ui: &mut egui::Ui, theme: &Theme, full_width: bool, text: &str) -> egui::Response {
    push_button(
        ui,
        full_width,
        text,
        GOLD.gamma_multiply(0.14),
        GOLD.gamma_multiply(0.20),
        theme.gold_ink,
    )
}

/// "Plain" outline button (transparent fill + hairline) — the calmest style.
pub fn outline_button(ui: &mut egui::Ui, theme: &Theme, full_width: bool, text: &str) -> egui::Response {
    let resp = push_button(ui, full_width, text, Color32::TRANSPARENT, theme.hover, theme.ink);
    ui.painter()
        .rect_stroke(resp.rect, 12.0, Stroke::new(1.0, theme.glass_border));
    resp
}

/// Small flat gold action pill for headers/toolbars ("New post").
pub fn pill_button(ui: &mut egui::Ui, _t: &Theme, label: &str) -> egui::Response {
    let resp = egui::Frame::none()
        .fill(GOLD)
        .rounding(12.0)
        .inner_margin(Margin::symmetric(14.0, 8.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(14.0)
                    .family(crate::icons::semibold())
                    .color(NAVY),
            )
        })
        .response
        .interact(Sense::click());
    let press = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("p"), resp.is_pointer_button_down_on(), 0.07);
    if press > 0.0 {
        ui.painter().rect_filled(
            egui::Rect::from_center_size(resp.rect.center(), resp.rect.size() * (1.0 - 0.03 * press)),
            12.0,
            GOLD_BRIGHT.gamma_multiply(press * 0.18),
        );
        ui.painter().text(
            resp.rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::new(14.0, crate::icons::semibold()),
            NAVY,
        );
    }
    resp
}

/// A round, frameless icon button: hover paints a soft circular wash.
pub fn icon_button(ui: &mut egui::Ui, theme: &Theme, glyph: &str, size: f32, color: Color32) -> egui::Response {
    let box_sz = size + 14.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(box_sz, box_sz), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(rect.center(), box_sz / 2.0, theme.hover);
    }
    let c = if resp.hovered() { theme.ink } else { color };
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(size), c);
    resp
}

/// A small recessed pill chip (label, optional accent color for the text).
pub fn chip(ui: &mut egui::Ui, theme: &Theme, label: &str, text_color: Color32) {
    egui::Frame::none()
        .fill(theme.hover) // recessed tonal
        .stroke(Stroke::new(1.0, theme.glass_border))
        .rounding(999.0)
        .inner_margin(Margin::symmetric(11.0, 4.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(12.0)
                    .family(crate::icons::medium())
                    .color(text_color),
            );
        });
}

/// Segmented control with a sliding neutral thumb (wallet chains, theme toggle,
/// filters). Gold is NOT used for the thumb (more iPadOS); selected text is `ink`.
/// Returns the newly-selected index when a different cell is clicked.
pub fn segmented(ui: &mut egui::Ui, theme: &Theme, id: &str, options: &[&str], selected: usize) -> Option<usize> {
    let h = 34.0;
    let n = options.len().max(1) as f32;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(120.0 * n), h), Sense::hover());
    // recessed track
    ui.painter().rect_filled(rect, 9.0, theme.hover);
    ui.painter()
        .rect_stroke(rect, 9.0, Stroke::new(1.0, theme.glass_border));
    let seg_w = rect.width() / n;
    let pos = ui
        .ctx()
        .animate_value_with_time(egui::Id::new(("seg", id)), selected as f32, 0.16);
    let thumb = egui::Rect::from_min_size(
        egui::pos2(rect.left() + pos * seg_w + 2.0, rect.top() + 2.0),
        egui::vec2(seg_w - 4.0, h - 4.0),
    );
    ui.painter().rect_filled(thumb, 7.0, theme.glass_fill); // neutral material thumb
    ui.painter()
        .rect_stroke(thumb, 7.0, Stroke::new(1.0, theme.glass_border));
    let mut out = None;
    for (i, opt) in options.iter().enumerate() {
        let cell = egui::Rect::from_min_size(
            egui::pos2(rect.left() + i as f32 * seg_w, rect.top()),
            egui::vec2(seg_w, h),
        );
        let r = ui.interact(cell, egui::Id::new(("segc", id, i)), Sense::click());
        let on = i == selected;
        let fam = if on {
            crate::icons::semibold()
        } else {
            egui::FontFamily::Proportional
        };
        ui.painter().text(
            cell.center(),
            Align2::CENTER_CENTER,
            *opt,
            FontId::new(13.0, fam),
            if on { theme.ink } else { theme.muted },
        );
        if r.clicked() && !on {
            out = Some(i);
        }
    }
    if (pos - selected as f32).abs() > 0.003 {
        ui.ctx().request_repaint();
    }
    out
}

/// iOS-style switch / toggle: recessed grey when off, GOLD when on, sliding knob.
pub fn switch(ui: &mut egui::Ui, theme: &Theme, id: &str, on: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(46.0, 28.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(egui::Id::new(("sw", id)), on, 0.14);
    let track = lerp(
        if theme.light {
            theme.glass_border
        } else {
            theme.surface2
        },
        GOLD,
        t,
    );
    ui.painter().rect_filled(rect, 14.0, track);
    if t < 0.5 {
        ui.painter()
            .rect_stroke(rect, 14.0, Stroke::new(1.0, theme.glass_border));
    }
    let kx = rect.left() + 14.0 + t * (rect.width() - 28.0);
    ui.painter()
        .circle_filled(egui::pos2(kx, rect.center().y), 11.0, Color32::WHITE);
    if (t - if on { 1.0 } else { 0.0 }).abs() > 0.003 {
        ui.ctx().request_repaint();
    }
    resp
}

/// Draw a gold focus ring (or a neutral hairline when unfocused) around an input.
pub fn input_ring(ui: &mut egui::Ui, theme: &Theme, resp: &egui::Response) {
    if resp.has_focus() {
        ui.painter()
            .rect_stroke(resp.rect.expand(1.0), 12.0, Stroke::new(1.5, GOLD));
        ui.painter()
            .rect_filled(resp.rect, 12.0, GOLD.gamma_multiply(0.06)); // faint wash
    } else {
        ui.painter()
            .rect_stroke(resp.rect, 12.0, Stroke::new(1.0, theme.glass_border));
    }
}

/// A prominent, social-app text input — large 17px text + generous internal
/// padding (so it reads as a real compose box, not a thin line) over the recessed
/// field fill + gold focus ring from the global Visuals. `rows`>1 → multiline.
/// Returns the `Response` (check `.changed()`/`.lost_focus()` at the call site).
pub fn field(ui: &mut egui::Ui, theme: &Theme, value: &mut String, hint: &str, rows: usize) -> egui::Response {
    // Draw our OWN visible box (distinct fill + real border, lightly-rounded so it
    // reads "square") with a frameless TextEdit inside — never an invisible line.
    let inner = egui::Frame::none()
        .fill(theme.field_fill())
        .stroke(Stroke::new(1.0, theme.field_border()))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(14.0, 12.0))
        .show(ui, |ui| {
            let te = if rows > 1 {
                egui::TextEdit::multiline(value).desired_rows(rows)
            } else {
                egui::TextEdit::singleline(value)
            };
            ui.add(
                te.frame(false)
                    .hint_text(hint)
                    .font(egui::FontId::proportional(17.0))
                    .margin(Margin::same(0.0))
                    .desired_width(f32::INFINITY),
            )
        });
    let resp = inner.inner;
    if resp.has_focus() {
        ui.painter()
            .rect_stroke(inner.response.rect, 10.0, Stroke::new(2.0, GOLD));
    }
    resp
}

/// A sized single-line variant of [`field`] for horizontal rows (chat compose,
/// search) — same visible box, but `width` is given so a send/close button can
/// sit beside it. Returns the inner TextEdit `Response`.
pub fn field_w(ui: &mut egui::Ui, theme: &Theme, value: &mut String, hint: &str, width: f32) -> egui::Response {
    let inner = egui::Frame::none()
        .fill(theme.field_fill())
        .stroke(Stroke::new(1.0, theme.field_border()))
        .rounding(10.0)
        .inner_margin(Margin::symmetric(12.0, 10.0))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(value)
                    .frame(false)
                    .hint_text(hint)
                    .font(egui::FontId::proportional(16.0))
                    .margin(Margin::same(0.0))
                    .desired_width((width - 24.0).max(40.0)),
            )
        });
    let resp = inner.inner;
    if resp.has_focus() {
        ui.painter()
            .rect_stroke(inner.response.rect, 10.0, Stroke::new(2.0, GOLD));
    }
    resp
}

/// A grouped iPad list row: rounded selection (gold tint) / neutral hover wash,
/// no left bar, no per-row hairline. The wash is pre-painted BEHIND the body so
/// text stays crisp. Returns the click response; `body` lays out the row content.
pub fn list_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    selected: bool,
    body: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    // Reserve the row rect first so we can paint the wash under the content.
    let inner = Margin::symmetric(12.0, 11.0); // ~44px min height
    let resp = egui::Frame::none()
        .inner_margin(inner)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        })
        .response
        .interact(Sense::click());
    let r = resp.rect;
    // Low-alpha selection / hover wash. egui paints later calls on top, but at this
    // alpha the wash reads fine behind the (already-laid-out) text, and dropping the
    // old 3px gold bar + bottom hairline is the whole point of the iPad list look.
    if selected {
        ui.painter().rect_filled(
            r,
            12.0,
            GOLD.gamma_multiply(if theme.light { 0.14 } else { 0.20 }),
        );
    } else if resp.hovered() {
        ui.painter().rect_filled(r, 12.0, theme.hover);
    }
    resp
}

/// Circular avatar: a media texture when a CID is present, else a deterministic
/// gradient initial (see `avatar_palette`).
#[allow(clippy::too_many_arguments)]
pub fn avatar(
    media: &mut MediaCache,
    engine: &Engine,
    ev_tx: &Sender<UiEvent>,
    ui: &mut egui::Ui,
    cid: &str,
    did: &str,
    size: f32,
) {
    if !cid.is_empty() {
        if let Some(tex) = media.texture(cid, engine, ev_tx) {
            ui.add(
                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                    .fit_to_exact_size(egui::vec2(size, size))
                    .rounding(size / 2.0),
            );
            return;
        }
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let c = rect.center();
    let r = size / 2.0;
    let (top, bottom) = avatar_palette(did);
    // Flat gradient disc — no gloss, no drop shadow (calm next to the neutral canvas).
    gradient_circle(ui.painter(), c, r, top, bottom);
    // Self-identity is the ONLY gold ring. The shell inserts the current user's DID
    // into ctx.data under "me-did" each frame, so we never need a signature change.
    let me = ui
        .ctx()
        .data(|d| d.get_temp::<String>(egui::Id::new("me-did")).unwrap_or_default());
    if !me.is_empty() && did == me {
        ui.painter()
            .circle_stroke(c, r - 1.0, Stroke::new(2.0, GOLD));
    }
    ui.painter().text(
        c,
        Align2::CENTER_CENTER,
        AppState::did_initial(did).to_string(),
        FontId::new(size * 0.42, crate::icons::semibold()),
        Color32::WHITE,
    );
}

/// A deterministic, muted color pair (gradient top, bottom) for a DID's avatar —
/// so each contact has a stable, recognisable color (Telegram-style) without
/// flooding the UI with gold.
pub fn avatar_palette(did: &str) -> (Color32, Color32) {
    const PAIRS: [(Color32, Color32); 8] = [
        (Color32::from_rgb(0x5B, 0x8D, 0xEF), Color32::from_rgb(0x3A, 0x63, 0xB8)), // blue
        (Color32::from_rgb(0x3F, 0xB8, 0xA8), Color32::from_rgb(0x2A, 0x83, 0x78)), // teal
        (Color32::from_rgb(0x9B, 0x7B, 0xE0), Color32::from_rgb(0x6E, 0x50, 0xB0)), // violet
        (Color32::from_rgb(0xE0, 0x7B, 0x9B), Color32::from_rgb(0xB0, 0x50, 0x70)), // rose
        (Color32::from_rgb(0xE0, 0x96, 0x63), Color32::from_rgb(0xB0, 0x6A, 0x40)), // orange
        (Color32::from_rgb(0x6F, 0xC5, 0x8A), Color32::from_rgb(0x4E, 0x9E, 0x6E)), // green
        (Color32::from_rgb(0x5B, 0xB8, 0xD0), Color32::from_rgb(0x3A, 0x86, 0xA0)), // cyan
        (Color32::from_rgb(0x7B, 0x85, 0xE0), Color32::from_rgb(0x50, 0x58, 0xB0)), // indigo
    ];
    let mut h: u32 = 2166136261;
    for b in did.bytes() {
        h = (h ^ b as u32).wrapping_mul(16777619);
    }
    let (top, bottom) = PAIRS[(h as usize) % PAIRS.len()];
    // Desaturate ~8% toward each color's luminance grey so the avatars sit calmly
    // next to the neutral canvas (modern iPad look) instead of reading as candy.
    (desaturate(top, 0.08), desaturate(bottom, 0.08))
}

/// Pull a color `amt` (0..1) toward its perceived-luminance grey.
fn desaturate(c: Color32, amt: f32) -> Color32 {
    let g = (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32).round();
    let f = |x: u8| (x as f32 + (g - x as f32) * amt).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

/// A vertically-gradient-filled disc (top→bottom), drawn as a triangle-fan mesh —
/// egui has no gradient fill primitive, so we build one. Used for avatars.
pub fn gradient_circle(painter: &egui::Painter, center: egui::Pos2, r: f32, top: Color32, bottom: Color32) {
    use egui::epaint::{Mesh, Vertex, WHITE_UV};
    let lerp = |a: Color32, b: Color32, t: f32| -> Color32 {
        let t = t.clamp(0.0, 1.0);
        let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
        Color32::from_rgba_premultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
    };
    let mut mesh = Mesh::default();
    let n: u32 = 48;
    mesh.vertices.push(Vertex { pos: center, uv: WHITE_UV, color: lerp(top, bottom, 0.5) });
    for i in 0..=n {
        let ang = (i as f32 / n as f32) * std::f32::consts::TAU;
        let p = center + egui::Vec2::angled(ang) * r;
        let ty = (p.y - (center.y - r)) / (2.0 * r);
        mesh.vertices.push(Vertex { pos: p, uv: WHITE_UV, color: lerp(top, bottom, ty) });
    }
    for i in 1..=n {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// A centered empty-state block: a soft tonal disc behind the glyph, then a
/// SemiBold title + muted subtitle.
pub fn empty_state(ui: &mut egui::Ui, theme: &Theme, glyph: &str, title: &str, sub: &str) {
    ui.add_space(80.0);
    ui.vertical_centered(|ui| {
        // soft disc behind the glyph
        let (disc, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), Sense::hover());
        ui.painter().circle_filled(disc.center(), 36.0, theme.hover);
        ui.painter().text(
            disc.center(),
            Align2::CENTER_CENTER,
            glyph,
            FontId::proportional(34.0),
            theme.faint,
        );
        ui.add_space(14.0);
        ui.label(
            RichText::new(title)
                .size(17.0)
                .family(crate::icons::semibold())
                .color(theme.ink),
        );
        ui.add_space(4.0);
        ui.label(RichText::new(sub).size(13.0).color(theme.muted));
    });
}

/// Wall-clock epoch milliseconds (post timestamps are epoch ms).
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Relative time (e.g. "3m", "2h", "5d") from a millisecond timestamp.
pub fn rel_time(ts_ms: i64, now_ms: i64) -> String {
    let d = (now_ms - ts_ms).max(0) / 1000;
    if d < 60 {
        "now".into()
    } else if d < 3600 {
        format!("{}m", d / 60)
    } else if d < 86400 {
        format!("{}h", d / 3600)
    } else {
        format!("{}d", d / 86400)
    }
}

