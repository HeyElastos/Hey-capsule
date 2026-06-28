//! Feed tab — full-fidelity port of the Android FeedScreen + PostCard.
//!
//! Each post is a frosted glass card: author header (tap a peer's avatar/name to
//! open their profile), a media carousel (‹ › / dots / "i/N" chip, tap a photo to
//! zoom, a ▶ placeholder for video), a heart + comment-count action row, the
//! caption, inline comments (top-level + indented replies) and an inline comment
//! composer with a reply target. Owners get a ⋯ menu (Edit caption / Delete).
//!
//! Borrow discipline: we clone the small per-iteration state we read so the App's
//! state is free for dispatch calls (`app.react`, `app.load_user`, …) and for the
//! disjoint media/engine borrows the avatar + image helpers need.

use egui::{Align, Color32, Layout, Margin, RichText, Sense, Stroke};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{AppState, ViewedUser};
use crate::theme::{Theme, GOLD, LIKE};

use super::{avatar, empty_state, rel_time};

const LIKE_EMOJI: &str = "❤️";

/// Desktop master-detail feed: a fixed-width scrollable list of compact post rows
/// (newest first) on the left, the full selected post (rendered by `post_card`) in
/// a scrollable reading pane on the right — an RSS/email-reader layout.
pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    if !app.state.feed_loaded {
        ui.add_space(80.0);
        ui.vertical_centered(|ui| ui.spinner());
        return;
    }
    if app.state.feed.is_empty() {
        empty_state(
            ui,
            theme,
            icons::PHOTO_CAMERA,
            "Your feed is empty",
            "Click “New post” to share your first photo.",
        );
        return;
    }

    // Bound the insert-only feed side-maps (reactions/comments/loaded_meta/carousel)
    // to the set of posts currently in the feed — cheap (the feed is already in RAM)
    // and prevents slow growth over a long session (spec §7 LOW). Done once per frame.
    let visible_ids: std::collections::HashSet<String> = app
        .state
        .feed
        .iter()
        .filter_map(|p| p.get("id").and_then(Value::as_str).map(str::to_owned))
        .collect();
    app.state.retain_posts(&visible_ids);

    let now = super::now_ms();

    // Responsive masonry grid. A lone post (or a normal-width window) stays one
    // centered, generous card; a wide/maximized window with several posts fans the
    // feed out into up to 3 column-balanced lanes so the space isn't wasted.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let avail = ui.available_width();
            let gap = 18.0_f32;
            let n = app.state.feed.len();

            let target = 540.0_f32; // preferred card width before adding a column
            let max_cols = 3usize;
            let cols = (((avail + gap) / (target + gap)).floor() as usize)
                .clamp(1, max_cols)
                .min(n.max(1));

            // Cap card width so cards stay readable on ultrawide; center the grid.
            let col_w = ((avail - gap * (cols as f32 - 1.0)) / cols as f32).min(720.0);
            let grid_w = col_w * cols as f32 + gap * (cols as f32 - 1.0);
            let pad = ((avail - grid_w) * 0.5).max(0.0);

            ui.horizontal_top(|ui| {
                ui.add_space(pad);
                for c in 0..cols {
                    if c > 0 {
                        ui.add_space(gap);
                    }
                    ui.allocate_ui_with_layout(
                        egui::vec2(col_w, 0.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_width(col_w);
                            // Round-robin so columns stay roughly balanced by count.
                            let mut i = c;
                            while i < n {
                                let post = app.state.feed[i].clone();
                                // Jumped here from the profile grid → scroll this post in.
                                let pid = post.get("id").and_then(Value::as_str).unwrap_or("");
                                if app.state.feed_scroll_to.as_deref() == Some(pid) {
                                    ui.scroll_to_cursor(Some(egui::Align::Min));
                                }
                                post_card(app, ui, theme, &post, now);
                                ui.add_space(14.0);
                                i += cols;
                            }
                        },
                    );
                }
            });
            ui.add_space(40.0);
        });

    // One-shot: consume the scroll target now that we've scrolled to it.
    app.state.feed_scroll_to = None;
}

fn post_card(app: &mut App, ui: &mut egui::Ui, theme: &Theme, post: &Value, now_ms: i64) {
    let id = post.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    if id.is_empty() {
        return;
    }
    let author = post.get("author").and_then(Value::as_str).unwrap_or("").to_string();
    let name = post.get("author_name").and_then(Value::as_str).unwrap_or("");
    let av_cid = post.get("author_avatar").and_then(Value::as_str).unwrap_or("");
    let caption = post.get("caption").and_then(Value::as_str).unwrap_or("");
    let ts = post.get("ts").and_then(Value::as_i64).unwrap_or(0);
    let media = post.get("media").and_then(Value::as_array).cloned().unwrap_or_default();
    let mine = !app.state.me_did.is_empty() && author == app.state.me_did;

    // Lazily load reactions + comments once per post id (so the counts populate).
    if !app.state.loaded_meta.contains(&id) {
        app.state.loaded_meta.insert(id.clone());
        app.load_reactions(&id);
        app.load_comments(&id);
    }

    // Reaction summary -> (liked, like_count).
    let (liked, like_count) = app
        .state
        .reactions
        .get(&id)
        .map(|r| {
            let liked = r.get("mine").and_then(Value::as_str) == Some(LIKE_EMOJI);
            let count = r
                .get("counts")
                .and_then(|c| c.get(LIKE_EMOJI))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            (liked, count)
        })
        .unwrap_or((false, 0));

    let comments: Vec<Value> = app
        .state
        .comments
        .get(&id)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let comment_count = comments.len();
    let box_open = app.state.open_comments.contains(&id);

    // Pending intents collected during the immediate-mode body, applied after the
    // glass frame closes (so no app.state borrow is live while we mutate it).
    let mut open_peer: Option<String> = None;
    let mut toggle_like = false;
    let mut toggle_comments = false;
    let mut zoom: Option<String> = None;
    let mut start_edit = false;
    let mut do_delete = false;
    let mut send_comment = false;
    let mut clear_reply = false;
    let mut set_reply: Option<(String, String)> = None;
    let mut open_tip = false;
    let mut copy_link = false;
    let mut mute_author = false;

    let card = theme.glass(18.0).show(ui, |ui| {
        // ── header ────────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let av = ui
                .scope(|ui| {
                    avatar(&mut app.media, &app.engine, &app.ev_tx, ui, av_cid, &author, 40.0);
                })
                .response
                .interact(Sense::click());
            // Avatar context menu (the "avatars anywhere" affordance) — non-own posts.
            if !mine && !author.is_empty() {
                av.context_menu(|ui| {
                    ui.set_min_width(170.0);
                    if super::menu_item(ui, theme, icons::PERSON, "View profile", "", false).clicked() {
                        open_peer = Some(author.clone());
                        ui.close_menu();
                    }
                    if super::menu_item(ui, theme, icons::PAID, "Send a tip", "", false).clicked() {
                        open_tip = true;
                        ui.close_menu();
                    }
                });
            }
            ui.add_space(10.0);
            ui.vertical(|ui| {
                let title = if name.is_empty() {
                    AppState::short_did(&author)
                } else {
                    name.to_string()
                };
                let nm = ui.add(
                    egui::Label::new(
                        RichText::new(title)
                            .size(15.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    )
                    .sense(Sense::click()),
                );
                if ts > 0 {
                    ui.label(RichText::new(rel_time(ts, now_ms)).size(11.0).color(theme.muted));
                }
                if !mine && (av.clicked() || nm.clicked()) {
                    open_peer = Some(author.clone());
                }
            });

            if mine {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.menu_button(RichText::new(icons::MORE_VERT).size(15.0).color(theme.muted), |ui| {
                        if ui.button(RichText::new("Edit caption").color(theme.ink)).clicked() {
                            start_edit = true;
                            ui.close_menu();
                        }
                        if ui.button(RichText::new("Delete post").color(LIKE)).clicked() {
                            do_delete = true;
                            ui.close_menu();
                        }
                    });
                });
            }
        });

        // ── media carousel ──────────────────────────────────────────────────────
        if !media.is_empty() {
            ui.add_space(10.0);
            if let Some(z) = carousel(app, ui, theme, &id, &media) {
                zoom = Some(z);
            }
            ui.add_space(10.0);
        } else {
            ui.add_space(8.0);
        }

        // ── action row (heart + comment) ────────────────────────────────────────
        ui.horizontal(|ui| {
            let (heart_glyph, heart_col) = if liked {
                (icons::FAVORITE, LIKE)
            } else {
                (icons::FAVORITE_BORDER, theme.ink)
            };
            // Heart with a scale-pop on like: clicking snaps the animated value to
            // 1.25 (keyed on post id); every frame it springs back toward 1.0.
            let pop_id = egui::Id::new(("heart-pop", &id));
            let pop = ui.ctx().animate_value_with_time(pop_id, 1.0, 0.18);
            let base = 22.0;
            let (hrect, hresp) = ui.allocate_exact_size(egui::vec2(base + 6.0, base + 6.0), Sense::click());
            let hresp = hresp.on_hover_text(if liked { "Unlike  L" } else { "Like  L" });
            ui.painter().text(
                hrect.center(),
                egui::Align2::CENTER_CENTER,
                heart_glyph,
                egui::FontId::proportional(base * pop),
                heart_col,
            );
            if pop > 1.005 {
                ui.ctx().request_repaint();
            }
            if hresp.clicked() {
                toggle_like = true;
                // Kick the pop: jump to 1.25 now so the next frame springs back to 1.0.
                ui.ctx().animate_value_with_time(pop_id, 1.25, 0.0);
            }
            if like_count > 0 {
                ui.add_space(6.0);
                ui.label(RichText::new(like_count.to_string()).size(14.0).color(theme.ink));
            }
            ui.add_space(18.0);
            let chat_col = if box_open { theme.gold_ink } else { theme.ink };
            if ui
                .add(egui::Label::new(RichText::new(icons::CHAT_BUBBLE_OUTLINE).size(20.0).color(chat_col)).sense(Sense::click()))
                .on_hover_text("Comment  C")
                .clicked()
            {
                toggle_comments = true;
            }
            if comment_count > 0 {
                ui.add_space(6.0);
                ui.label(RichText::new(comment_count.to_string()).size(14.0).color(theme.ink));
            }
            // Tip the author by identity (non-own posts only) — opens the Tip sheet.
            if !mine && !author.is_empty() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add(egui::Label::new(RichText::new(icons::PAID).size(20.0).color(theme.gold_ink)).sense(Sense::click()))
                        .on_hover_text("Send a tip")
                        .clicked()
                    {
                        open_tip = true;
                    }
                });
            }
        });

        // ── caption ─────────────────────────────────────────────────────────────
        if !caption.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(caption).size(15.0).color(theme.ink));
        }

        // ── inline edit field (owner) ───────────────────────────────────────────
        if let Some((eid, draft)) = app.state.editing.clone() {
            if eid == id {
                ui.add_space(8.0);
                theme.glass(12.0).show(ui, |ui| {
                    ui.label(
                        RichText::new("Edit caption")
                            .size(12.0)
                            .family(icons::semibold())
                            .color(theme.gold_ink),
                    );
                    ui.add_space(6.0);
                    let mut text = draft.clone();
                    let resp = super::field(ui, theme, &mut text, "Write a caption…", 2);
                    if resp.changed() {
                        app.state.editing = Some((id.clone(), text.clone()));
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if super::outline_button(ui, theme, false, "Cancel").clicked() {
                            app.state.editing = None;
                        }
                        ui.add_space(8.0);
                        if super::primary_button(ui, false, "Save").clicked() {
                            app.edit_post(&id, &text);
                            app.state.editing = None;
                        }
                    });
                });
            }
        }

        // ── existing comments (top-level + indented replies) ────────────────────
        if !comments.is_empty() {
            ui.add_space(10.0);
            let tops: Vec<&Value> = comments.iter().filter(|c| parent_of(c).is_empty()).collect();
            for c in tops {
                let cid = c.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                if let Some(t) = comment_row(ui, theme, c, false) {
                    set_reply = Some(t);
                }
                for r in comments.iter().filter(|r| parent_of(r) == cid) {
                    let _ = comment_row(ui, theme, r, true);
                }
            }
        }

        // ── inline comment composer (opens on tap / reply) ──────────────────────
        let reply = app.state.reply_to.get(&id).cloned();
        if box_open || reply.is_some() {
            if let Some((_, label)) = &reply {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Replying to {label}"))
                            .size(11.0)
                            .color(theme.gold_ink),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button(RichText::new("Cancel").size(11.0).color(theme.muted)).clicked() {
                            clear_reply = true;
                        }
                    });
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let draft = app.state.comment_draft.entry(id.clone()).or_default();
                let hint = if reply.is_some() { "Reply…" } else { "Add a comment…" };
                // Shared visible field (gold focus ring) + a small gold "Send" pill,
                // so the inline composer matches the chat compose row.
                let fw = (ui.available_width() - 64.0).max(80.0);
                let resp = super::field_w(ui, theme, draft, hint, fw);
                let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.add_space(4.0);
                if (super::pill_button(ui, theme, "Send").clicked() || enter)
                    && !app.state.comment_draft.get(&id).map(|s| s.trim().is_empty()).unwrap_or(true)
                {
                    send_comment = true;
                }
            });
        }
    });

    // Right-click the post card → a desktop context menu (Like / Comment / Copy link
    // / View author / Mute author / Delete-own). Wires the same intents the inline
    // controls do; "Copy link" + "Mute author" are label-only (no wired action yet).
    card.response.interact(Sense::click()).context_menu(|ui| {
        ui.set_min_width(180.0);
        let (lglyph, llabel) = if liked {
            (icons::FAVORITE, "Unlike")
        } else {
            (icons::FAVORITE_BORDER, "Like")
        };
        if super::menu_item(ui, theme, lglyph, llabel, "L", false).clicked() {
            toggle_like = true;
            ui.close_menu();
        }
        if super::menu_item(ui, theme, icons::CHAT_BUBBLE_OUTLINE, "Comment", "C", false).clicked() {
            toggle_comments = true;
            ui.close_menu();
        }
        if super::menu_item(ui, theme, icons::LINK, "Copy link", "", false).clicked() {
            copy_link = true;
            ui.close_menu();
        }
        if !mine && !author.is_empty() {
            if super::menu_item(ui, theme, icons::PERSON, "View author", "", false).clicked() {
                open_peer = Some(author.clone());
                ui.close_menu();
            }
            if super::menu_item(ui, theme, icons::NOTIFICATIONS_OFF, "Mute author", "", false).clicked() {
                mute_author = true;
                ui.close_menu();
            }
        }
        if mine {
            ui.add_space(2.0);
            if super::menu_item(ui, theme, icons::EDIT, "Edit caption", "", false).clicked() {
                start_edit = true;
                ui.close_menu();
            }
            if super::menu_item(ui, theme, icons::DELETE, "Delete post", "", true).clicked() {
                do_delete = true;
                ui.close_menu();
            }
        }
    });

    // ── apply collected intents (no app.state borrow live here) ─────────────────
    if copy_link {
        ui.ctx().output_mut(|o| o.copied_text = format!("hey://post/{id}"));
        let now = ui.ctx().input(|i| i.time);
        app.toast("Link copied", now);
    }
    if mute_author {
        let now = ui.ctx().input(|i| i.time);
        app.toast("Mute author coming soon", now);
    }
    if let Some(d) = open_peer {
        app.state.viewed = Some(ViewedUser { did: d.clone(), ..Default::default() });
        app.load_user(&d);
    }
    if open_tip {
        app.open_tip(&author, name);
    }
    if toggle_like {
        app.react(&id, LIKE_EMOJI);
    }
    if toggle_comments {
        if app.state.open_comments.contains(&id) {
            app.state.open_comments.remove(&id);
            app.state.reply_to.remove(&id);
        } else {
            app.state.open_comments.insert(id.clone());
        }
    }
    if let Some(cid) = zoom {
        app.state.zoom_cid = Some(cid);
    }
    if start_edit {
        app.state.editing = Some((id.clone(), caption.to_string()));
    }
    if do_delete {
        app.delete_post(&id);
    }
    if let Some(t) = set_reply {
        app.state.reply_to.insert(id.clone(), t);
        app.state.open_comments.insert(id.clone());
    }
    if clear_reply {
        app.state.reply_to.remove(&id);
    }
    if send_comment {
        let text = app
            .state
            .comment_draft
            .get(&id)
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if !text.is_empty() {
            let parent = app
                .state
                .reply_to
                .get(&id)
                .map(|(cid, _)| cid.clone())
                .unwrap_or_default();
            app.add_comment(&id, &text, &parent);
            app.state.comment_draft.insert(id.clone(), String::new());
            app.state.reply_to.remove(&id);
            app.state.open_comments.remove(&id);
        }
    }
}

fn parent_of(c: &Value) -> String {
    c.get("parent").and_then(Value::as_str).unwrap_or("").to_string()
}

/// One comment line: "Author  text", with a "Reply" affordance on top-level rows.
/// Returns Some((comment_id, author label)) when the user taps Reply.
fn comment_row(ui: &mut egui::Ui, theme: &Theme, c: &Value, indent: bool) -> Option<(String, String)> {
    let cid = c.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let author = c.get("author").and_then(Value::as_str).unwrap_or("");
    let name = c.get("author_name").and_then(Value::as_str).unwrap_or("");
    let text = c.get("text").and_then(Value::as_str).unwrap_or("");
    let label = if name.is_empty() {
        AppState::short_did(author)
    } else {
        name.to_string()
    };
    let mut reply: Option<(String, String)> = None;
    ui.horizontal(|ui| {
        if indent {
            ui.add_space(26.0);
        }
        ui.add(egui::Label::new(
            RichText::new(format!("{label}  ")).size(13.0).strong().color(theme.gold_ink),
        ).wrap());
        ui.add(egui::Label::new(RichText::new(text).size(14.0).color(theme.ink)).wrap());
        if !indent {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button(RichText::new("Reply").size(11.0).color(theme.muted)).clicked() {
                    reply = Some((cid.clone(), label.clone()));
                }
            });
        }
    });
    reply
}

/// Media carousel: a single selected tile with ‹ › arrows + dots + an "i/N" chip
/// when there is more than one. Returns Some(cid) when a photo is tapped (zoom).
fn carousel(
    app: &mut App,
    ui: &mut egui::Ui,
    theme: &Theme,
    post_id: &str,
    media: &[Value],
) -> Option<String> {
    let count = media.len();
    let idx = (*app.state.carousel.get(post_id).unwrap_or(&0)).min(count.saturating_sub(1));
    let tile = &media[idx];
    let cid = tile.get("cid").and_then(Value::as_str).unwrap_or("").to_string();
    let is_video = tile.get("type").and_then(Value::as_str) == Some("video");

    let mut tapped: Option<String> = None;
    let mut next_idx: Option<usize> = None;

    // The active tile.
    if is_video {
        video_placeholder(ui, theme);
    } else if let Some(tex) = app.media.texture(&cid, &app.engine, &app.ev_tx) {
        // Full-bleed: extend into the card's ~14px side margins so the image spans
        // the whole card width (Instagram-style). Portrait images letterbox centered.
        let m = 14.0;
        let content = ui.max_rect();
        let full_w = content.width() + 2.0 * m;
        let size = tex.size_vec2();
        let aspect = size.y.max(1.0) / size.x.max(1.0);
        let mut w = full_w;
        let mut h = w * aspect;
        if h > 480.0 {
            h = 480.0;
            w = h / aspect;
        }
        let top = ui.cursor().top();
        let outer = egui::Rect::from_min_size(egui::pos2(content.left() - m, top), egui::vec2(full_w, h));
        let resp = ui
            .allocate_ui_at_rect(outer, |ui| {
                ui.set_clip_rect(outer);
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                            .fit_to_exact_size(egui::vec2(w, h))
                            .sense(Sense::click()),
                    )
                })
                .inner
            })
            .inner;
        if resp.clicked() {
            if count > 1 {
                next_idx = Some((idx + 1) % count);
            } else {
                tapped = Some(cid.clone());
            }
        }
        if resp.secondary_clicked() {
            tapped = Some(cid.clone());
        }
    } else {
        media_placeholder(ui, theme);
    }

    // Multi-tile chrome: ‹ › arrows, the "i/N" chip, and a row of dots.
    if count > 1 {
        ui.horizontal(|ui| {
            if ui.button(RichText::new(icons::ARROW_BACK).size(18.0).color(theme.ink)).clicked() {
                next_idx = Some((idx + count - 1) % count);
            }
            // "i/N" chip: a recessed material pill on light, a dark scrim pill on dark.
            let (chip_fill, chip_stroke, chip_text) = if theme.light {
                (theme.glass_fill, Stroke::new(1.0, theme.glass_border), theme.muted)
            } else {
                (Color32::from_black_alpha(110), Stroke::NONE, Color32::WHITE)
            };
            egui::Frame::none()
                .fill(chip_fill)
                .stroke(chip_stroke)
                .rounding(10.0)
                .inner_margin(Margin::symmetric(8.0, 3.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("{}/{}", idx + 1, count))
                            .size(11.0)
                            .color(chip_text),
                    );
                });
            if ui.button(RichText::new(icons::CHEVRON_RIGHT).size(18.0).color(theme.ink)).clicked() {
                next_idx = Some((idx + 1) % count);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !is_video {
                    if ui.small_button(RichText::new(format!("{}  Zoom", icons::SEARCH)).size(11.0).color(theme.muted)).clicked() {
                        tapped = Some(cid.clone());
                    }
                }
            });
        });
        // Dots.
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - (count as f32 * 12.0)).max(0.0) * 0.5);
            for i in 0..count {
                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), Sense::hover());
                let (col, rad) = if i == idx {
                    (GOLD, 4.0)
                } else {
                    (theme.faint, 3.0)
                };
                ui.painter().circle_filled(r.center(), rad, col);
            }
        });
    }

    if let Some(ni) = next_idx {
        app.state.carousel.insert(post_id.to_string(), ni);
    }
    tapped
}

fn media_placeholder(ui: &mut egui::Ui, theme: &Theme) {
    // Still-loading photo tile: a recessed material rect (calm on both themes),
    // not a black-alpha scrim that punches a hole in the flat canvas.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 220.0), Sense::hover());
    ui.painter().rect_filled(rect, 14.0, theme.hover);
    ui.painter()
        .rect_stroke(rect, 14.0, Stroke::new(1.0, theme.glass_border));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "…",
        egui::FontId::proportional(28.0),
        theme.faint,
    );
}

fn video_placeholder(ui: &mut egui::Ui, theme: &Theme) {
    // Recessed material tile (calm on both themes), matching `media_placeholder` —
    // not a black scrim that punches a hole in the flat canvas. The gold play glyph
    // reads as the "video" affordance.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 240.0), Sense::hover());
    ui.painter().rect_filled(rect, 14.0, theme.hover);
    ui.painter()
        .rect_stroke(rect, 14.0, Stroke::new(1.0, theme.glass_border));
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icons::PLAY_CIRCLE,
        egui::FontId::proportional(56.0),
        theme.gold_ink,
    );
}

/// Full-screen pinch/zoom viewer for a posted photo. Shown over a near-black
/// overlay when `app.state.zoom_cid` is Some; mouse-wheel scales, ✕ clears it.
pub fn zoom_viewer(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let Some(cid) = app.state.zoom_cid.clone() else {
        return;
    };

    // A scale value persisted in egui's per-frame memory keyed on the cid, so it
    // resets when a new image opens and survives between frames while open.
    let scale_id = egui::Id::new(("zoom-scale", &cid));
    let mut scale: f32 = ctx.memory(|m| m.data.get_temp(scale_id).unwrap_or(1.0_f32));

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("zoom-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let rect = screen;
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 245));
            // Block clicks from reaching content beneath.
            let bg = ui.allocate_rect(rect, Sense::click());

            // Mouse-wheel zoom.
            let hovered = ui.rect_contains_pointer(rect);
            if hovered {
                let dy = ctx.input(|i| i.raw_scroll_delta.y);
                if dy.abs() > 0.0 {
                    scale = (scale * (1.0 + dy * 0.0015)).clamp(1.0, 5.0);
                }
            }

            // The image, centered and scaled.
            if let Some(tex) = app.media.texture(&cid, &app.engine, &app.ev_tx) {
                let size = tex.size_vec2();
                let max_w = rect.width() * 0.96;
                let max_h = rect.height() * 0.90;
                let fit = (max_w / size.x).min(max_h / size.y).min(4.0);
                let draw = size * fit * scale;
                let img_rect = egui::Rect::from_center_size(rect.center(), draw);
                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                    .paint_at(ui, img_rect.intersect(rect));
            } else {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "…",
                    egui::FontId::proportional(34.0),
                    Color32::WHITE,
                );
            }

            // Close button (top-right).
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 56.0, rect.top() + 16.0),
                egui::vec2(40.0, 40.0),
            );
            // Material circle close (surface2), an iPad-sheet dismiss affordance over
            // the dark backdrop instead of a black-alpha chip.
            let close = ui.put(
                close_rect,
                egui::Button::new(RichText::new(icons::CLOSE).size(20.0).color(theme.ink))
                    .fill(theme.surface2)
                    .stroke(Stroke::new(1.0, theme.glass_border))
                    .rounding(20.0),
            );

            // Hint.
            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 24.0),
                egui::Align2::CENTER_CENTER,
                "Scroll to zoom",
                egui::FontId::proportional(12.0),
                Color32::from_white_alpha(150),
            );

            if close.clicked()
                || (bg.clicked() && scale <= 1.001)
                || ctx.input(|i| i.key_pressed(egui::Key::Escape))
            {
                app.state.zoom_cid = None;
                ctx.memory_mut(|m| m.data.remove::<f32>(scale_id));
                return;
            }
            ctx.memory_mut(|m| m.data.insert_temp(scale_id, scale));
        });
}
