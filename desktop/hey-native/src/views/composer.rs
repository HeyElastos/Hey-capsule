//! ComposerScreen — create a post. Desktop two-pane "New post" dialog (the
//! Instagram-web shape): media on the LEFT, caption + share on the RIGHT. Shown
//! when `app.state.modal == Some(Modal::Composer)`.
//!
//! Left pane: a big glass tap-target that opens the multi-file picker; once tiles
//! exist, a contained main preview (photo via the media cache, ▶ for video) with a
//! ✕ remove and a horizontal thumbnail strip + Add tile. Right pane: a tall caption
//! field, a gold "Share post" button (→ `app.create_post`), status, and the
//! provenance footer. The picker, upload and post all dispatch on engine workers;
//! the foundation closes the modal + reloads the feed when the Posted event lands.

use egui::{Color32, RichText, Sense, Stroke};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::Modal;
use crate::theme::{Theme, GOLD, NAVY};

pub fn ui(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if app.state.modal != Some(Modal::Composer) {
        return;
    }

    let screen = ctx.screen_rect();
    // Landscape desktop dialog — wide, sized to content (no full-height stretch).
    let w = (screen.width() - 80.0).clamp(560.0, 920.0);
    let pane_h = (screen.height() - 240.0).clamp(360.0, 540.0);

    egui::Window::new("composer")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width(w);

            // ── header ──────────────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("New post")
                        .size(18.0)
                        .family(crate::icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        app.state.modal = None;
                    }
                });
            });
            ui.add_space(2.0);
            ui.painter().hline(
                ui.max_rect().x_range(),
                ui.cursor().top(),
                Stroke::new(1.0, theme.glass_border),
            );
            ui.add_space(14.0);

            // ── two panes: media | details ──────────────────────────────────────
            let gap = 20.0;
            let media_w = ((w - gap) * 0.54).round();
            let detail_w = w - media_w - gap;

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(media_w, pane_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(media_w);
                        media_pane(app, ui, theme, pane_h);
                    },
                );

                // faint divider
                let x = ui.cursor().left() + gap * 0.5;
                ui.painter().vline(
                    x,
                    egui::Rangef::new(ui.cursor().top(), ui.cursor().top() + pane_h),
                    Stroke::new(1.0, theme.glass_border),
                );
                ui.add_space(gap);

                ui.allocate_ui_with_layout(
                    egui::vec2(detail_w, pane_h),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(detail_w);
                        detail_pane(app, ui, theme, pane_h);
                    },
                );
            });
        });
}

// ── left: media ─────────────────────────────────────────────────────────────────
fn media_pane(app: &mut App, ui: &mut egui::Ui, theme: &Theme, pane_h: f32) {
    let tiles = app.state.composer.tiles.clone();
    let busy = app.state.composer.busy;
    if tiles.is_empty() {
        empty_picker(app, ui, theme, busy, pane_h);
    } else {
        media_preview(app, ui, theme, &tiles, busy, pane_h);
    }
}

// ── right: caption + share ────────────────────────────────────────────────────────
fn detail_pane(app: &mut App, ui: &mut egui::Ui, theme: &Theme, pane_h: f32) {
    let tiles = app.state.composer.tiles.clone();
    let busy = app.state.composer.busy;

    ui.label(
        RichText::new("Add a few photos or a video, then a caption.")
            .size(13.0)
            .color(theme.muted),
    );
    ui.add_space(12.0);

    // Caption fills the slack between the subtitle and the bottom button row, routed
    // through the shared field kit (visible box + gold focus ring) so it matches every
    // other compose input. Rows are derived from the slack height (~24px/line).
    let footer_reserve = 150.0;
    let cap_h = (pane_h - 32.0 - footer_reserve).max(120.0);
    let rows = ((cap_h / 24.0).floor() as usize).max(4);
    super::field(ui, theme, &mut app.state.composer.caption, "Write a caption…", rows);

    ui.add_space(12.0);

    // status line (compact)
    let status = app.state.composer.status.clone();
    if !status.is_empty() {
        ui.label(RichText::new(status).size(12.0).color(theme.muted));
        ui.add_space(8.0);
    }

    // ── footer: buttons + provenance ────────────────────────────────────────────
    let can_share = !tiles.is_empty();
    if busy {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.add_space(8.0);
            ui.label(RichText::new("Publishing…").size(15.0).color(theme.gold_ink));
        });
    } else {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Share = the shared gold primary; when no media is picked it reads as a
            // dimmed-gold disabled capsule (same shape, no click) instead of a bare
            // egui button so the dialog matches the rest of the design system.
            let shared = if can_share {
                super::primary_button(ui, false, "Share post").clicked()
            } else {
                super::push_button(
                    ui,
                    false,
                    "Share post",
                    GOLD.gamma_multiply(0.4),
                    GOLD.gamma_multiply(0.4),
                    NAVY.gamma_multiply(0.7),
                );
                false
            };
            if shared {
                app.state.composer.busy = true;
                app.state.composer.status = "Publishing…".into();
                let caption = app.state.composer.caption.clone();
                app.create_post(caption, tiles.clone());
            }
            ui.add_space(8.0);
            if super::outline_button(ui, theme, false, "Cancel").clicked() {
                app.state.modal = None;
            }
        });
    }

    ui.add_space(12.0);
    ui.label(
        RichText::new("Pinned on-device · signed · federated via Carrier")
            .size(11.0)
            .color(theme.muted),
    );
}

/// The big "tap to add" glass target shown when nothing is picked yet — fills the
/// media pane height and centers its prompt.
fn empty_picker(app: &mut App, ui: &mut egui::Ui, theme: &Theme, busy: bool, h: f32) {
    let resp = theme
        .glass(16.0)
        .show(ui, |ui| {
            ui.set_min_height(h - 8.0);
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space((h * 0.5 - 64.0).max(16.0));
                ui.label(RichText::new(icons::ADD_PHOTO_ALTERNATE).size(48.0).color(theme.gold_ink));
                ui.add_space(8.0);
                ui.label(
                    RichText::new("Click to add photos or video")
                        .size(15.0)
                        .family(crate::icons::semibold())
                        .color(theme.ink),
                );
                ui.label(RichText::new("Up to 10 per post").size(12.0).color(theme.muted));
            });
        })
        .response
        .interact(Sense::click());
    if resp.clicked() && !busy {
        app.state.composer.busy = true;
        app.state.composer.status = "Reading…".into();
        app.pick_media();
    }
}

/// Contained main preview that fills the pane, plus a horizontal thumbnail strip
/// (selected tile highlighted in gold, click to select, ✕ to remove) and an "Add"
/// tile at the end while under the 10-media cap.
fn media_preview(app: &mut App, ui: &mut egui::Ui, theme: &Theme, tiles: &[Value], busy: bool, pane_h: f32) {
    let count = tiles.len();
    let sel_id = egui::Id::new("composer-page");
    let mut idx: usize = ui.ctx().memory(|m| m.data.get_temp(sel_id).unwrap_or(0usize));
    idx = idx.min(count.saturating_sub(1));

    let tile = &tiles[idx];
    let cid = tile.get("cid").and_then(Value::as_str).unwrap_or("").to_string();
    let is_video = tile.get("type").and_then(Value::as_str) == Some("video");

    let mut remove_idx: Option<usize> = None;
    let mut next_idx = idx;

    // ── main preview box (fills the pane minus the thumbnail strip) ──────────────
    let frame_w = ui.available_width();
    let main_h = (pane_h - 56.0 - 12.0 - 18.0).max(160.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(frame_w, main_h), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 16.0, theme.glass_fill);
    painter.rect_stroke(rect, 16.0, Stroke::new(1.0, theme.glass_border));
    if is_video {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            icons::PLAY_CIRCLE,
            egui::FontId::proportional(60.0),
            theme.gold_ink,
        );
        painter.text(
            egui::pos2(rect.center().x, rect.center().y + 44.0),
            egui::Align2::CENTER_CENTER,
            "Video",
            egui::FontId::proportional(13.0),
            theme.muted,
        );
    } else if let Some(tex) = app.media.texture(&cid, &app.engine, &app.ev_tx) {
        let inner = rect.shrink(8.0);
        ui.allocate_ui_at_rect(inner, |ui| {
            ui.set_clip_rect(inner);
            ui.vertical_centered(|ui| {
                ui.add(
                    egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                        .max_width(inner.width())
                        .max_height(inner.height())
                        .rounding(10.0),
                );
            });
        });
    } else {
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "…",
            egui::FontId::proportional(28.0),
            theme.muted,
        );
    }
    // remove (✕) on the main preview (top-right)
    let close_rect = egui::Rect::from_min_size(rect.right_top() + egui::vec2(-40.0, 8.0), egui::vec2(30.0, 30.0));
    if ui
        .put(
            close_rect,
            egui::Button::new(RichText::new(icons::CLOSE).size(15.0).color(Color32::WHITE))
                .fill(Color32::from_black_alpha(130))
                .rounding(15.0),
        )
        .clicked()
    {
        remove_idx = Some(idx);
    }

    // ── thumbnail strip ───────────────────────────────────────────────────────
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        let thumb = 56.0;
        for (i, t) in tiles.iter().enumerate() {
            let tcid = t.get("cid").and_then(Value::as_str).unwrap_or("").to_string();
            let tvid = t.get("type").and_then(Value::as_str) == Some("video");
            let (tr, tresp) = ui.allocate_exact_size(egui::vec2(thumb, thumb), Sense::click());
            let tp = ui.painter_at(tr);
            tp.rect_filled(tr, 8.0, theme.glass_fill);
            if tvid {
                tp.text(tr.center(), egui::Align2::CENTER_CENTER, icons::PLAY_ARROW, egui::FontId::proportional(22.0), theme.muted);
            } else if let Some(tex) = app.media.texture(&tcid, &app.engine, &app.ev_tx) {
                let s = tex.size_vec2();
                let f = (thumb / s.x.max(1.0)).max(thumb / s.y.max(1.0));
                let d = s * f;
                let ir = egui::Rect::from_center_size(tr.center(), d);
                tp.image(tex.id(), ir, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            } else {
                tp.text(tr.center(), egui::Align2::CENTER_CENTER, "…", egui::FontId::proportional(16.0), theme.muted);
            }
            let (bw, bc) = if i == idx { (2.0, GOLD) } else { (1.0, theme.glass_border) };
            ui.painter().rect_stroke(tr, 8.0, Stroke::new(bw, bc));
            if tresp.clicked() {
                next_idx = i;
            }
        }
        if count < 10 {
            let (ar, aresp) = ui.allocate_exact_size(egui::vec2(56.0, 56.0), Sense::click());
            let hov = aresp.hovered();
            ui.painter().rect_filled(ar, 8.0, if hov { theme.hover } else { Color32::TRANSPARENT });
            ui.painter().rect_stroke(ar, 8.0, Stroke::new(1.5, theme.glass_border));
            ui.painter().text(ar.center(), egui::Align2::CENTER_CENTER, icons::ADD, egui::FontId::proportional(24.0), theme.gold_ink);
            if aresp.clicked() && !busy {
                app.state.composer.busy = true;
                app.state.composer.status = "Reading…".into();
                app.pick_media();
            }
        }
    });
    ui.add_space(4.0);
    ui.label(
        RichText::new(format!("{count}/10 · click a photo to preview, ✕ to remove"))
            .size(11.0)
            .color(theme.muted),
    );

    // ── apply mutations ───────────────────────────────────────────────────────
    if let Some(ri) = remove_idx {
        if ri < app.state.composer.tiles.len() {
            app.state.composer.tiles.remove(ri);
        }
        let new_len = app.state.composer.tiles.len();
        next_idx = if new_len == 0 { 0 } else { ri.min(new_len - 1) };
        app.state.composer.status = format!("{new_len} selected");
    }
    ui.ctx().memory_mut(|m| m.data.insert_temp(sel_id, next_idx));
}
