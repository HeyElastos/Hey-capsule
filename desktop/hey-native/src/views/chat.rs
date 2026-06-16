//! Chat tab — full-fidelity port of the Android ChatListScreen + ConversationScreen.
//!
//! List: DM contacts + groups merged into one time-sorted set of glass rows
//! (46px gradient avatar, name, last preview, trailing rel-time + unread badge),
//! a long-press/✕ delete confirm, two FABs (new group / add contact), empty state.
//! Conversation: back/avatar/name/search header, message bubbles with inline image
//! + file attachments and reaction chips, an emoji picker on long-press, and a
//! compose row (attach / text / send). The two add/group sheets live in
//! `chat_sheets` and are surfaced via `app.state.modal`.

use egui::{Align, Align2, Color32, FontId, Layout, Margin, RichText, Sense, Stroke};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{Modal, OpenChat, UiEvent};
use crate::theme::{Theme, GOLD, GOLD2, GOLD_BRIGHT, LIKE, NAVY};

use super::{empty_state, icon_button, list_row, now_ms, rel_time, segmented};

const EMOJI: [&str; 8] = ["👍", "❤️", "😂", "😮", "😢", "🎉", "🙏", "🔥"];

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // Desktop two-pane: a fixed-width conversation list on the left, the open
    // conversation (or an empty hint) on the right.
    let avail_h = ui.available_height();
    let list_w = 300.0_f32;
    ui.horizontal_top(|ui| {
        // ── left: conversation list ──
        ui.allocate_ui_with_layout(
            egui::vec2(list_w, avail_h),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(avail_h);
                list_toolbar(app, ui, theme);
                egui::ScrollArea::vertical()
                    .id_source("chat-list-pane")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        list(app, ui, theme);
                    });
            },
        );
        // ── divider ──
        ui.add_space(10.0);
        let sep_x = ui.cursor().left();
        ui.painter().vline(
            sep_x,
            egui::Rangef::new(ui.min_rect().top(), ui.min_rect().top() + avail_h),
            Stroke::new(1.0, theme.glass_border),
        );
        ui.add_space(10.0);
        // ── right: conversation or empty hint ──
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), avail_h),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(avail_h);
                if let Some(chat) = app.state.open_chat.clone() {
                    conversation(app, ui, theme, &chat);
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(avail_h * 0.30);
                        // soft tonal disc behind the glyph (matches empty_state)
                        let (disc, _) = ui.allocate_exact_size(egui::vec2(72.0, 72.0), Sense::hover());
                        ui.painter().circle_filled(disc.center(), 36.0, theme.hover);
                        ui.painter().text(
                            disc.center(),
                            Align2::CENTER_CENTER,
                            icons::FORUM,
                            FontId::proportional(34.0),
                            theme.faint,
                        );
                        ui.add_space(14.0);
                        ui.label(
                            RichText::new("Select a conversation")
                                .size(17.0)
                                .family(icons::semibold())
                                .color(theme.ink),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("…or start a new one with “New chat”")
                                .size(13.0)
                                .color(theme.muted),
                        );
                    });
                }
            },
        );
    });
}

// ── list ──────────────────────────────────────────────────────────────────────

fn list(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // Merge DM contacts + groups into one set of rows, newest activity first.
    struct Row {
        chat: OpenChat,
        preview: String,
        ts: i64,
        unread: i64,
    }
    let mut rows: Vec<Row> = Vec::new();
    for c in app.state.contacts.clone() {
        let did = c.get("did").and_then(Value::as_str).unwrap_or("").to_string();
        // Hide blocked DM contacts (Block & remove in the chat-info sheet) — exactly
        // like Android's `.filter { it.isGroup || it.id !in blocked }`.
        if app.state.blocked_dids.contains(&did) {
            continue;
        }
        let name = c
            .get("name")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| crate::state::AppState::short_did(&did));
        let preview = c
            .get("lastPreview")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let ts = c.get("lastTs").and_then(Value::as_i64).unwrap_or(0);
        let unread = c.get("unread").and_then(Value::as_i64).unwrap_or(0);
        rows.push(Row {
            chat: OpenChat { id: did, name, is_group: false },
            preview,
            ts,
            unread,
        });
    }
    for g in app.state.groups.clone() {
        let gid = g.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        let name = g.get("name").and_then(Value::as_str).unwrap_or("Group").to_string();
        let members = g.get("members").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
        let preview = g
            .get("lastPreview")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{members} members"));
        let ts = g.get("lastTs").and_then(Value::as_i64).unwrap_or(0);
        let unread = g.get("unread").and_then(Value::as_i64).unwrap_or(0);
        rows.push(Row {
            chat: OpenChat { id: gid, name, is_group: true },
            preview,
            ts,
            unread,
        });
    }
    rows.sort_by(|a, b| b.ts.cmp(&a.ts));

    // "All" / "Unread" segmented filter above the list (transient, not persisted).
    let filter_id = egui::Id::new("chat-list-filter");
    let mut filter = ui.ctx().data(|d| d.get_temp::<usize>(filter_id).unwrap_or(0));
    if let Some(sel) = segmented(ui, theme, "chat-filter", &["All", "Unread"], filter) {
        filter = sel;
        ui.ctx().data_mut(|d| d.insert_temp(filter_id, sel));
    }
    if filter == 1 {
        rows.retain(|r| r.unread > 0);
    }
    ui.add_space(8.0);

    if app.state.chats_loaded && rows.is_empty() {
        empty_state(
            ui,
            theme,
            icons::CHAT_BUBBLE_OUTLINE,
            if filter == 1 { "All caught up" } else { "No conversations yet" },
            if filter == 1 {
                "No unread conversations right now."
            } else {
                "Use “New chat” above to message someone you follow, or paste a friend link."
            },
        );
        return;
    }

    let now = now_ms();
    let open_id = app.state.open_chat.as_ref().map(|c| c.id.clone());
    for row in rows {
        let selected = open_id.as_deref() == Some(row.chat.id.as_str());
        let resp = list_row(ui, theme, selected, |ui| {
            ui.horizontal(|ui| {
                gradient_avatar(ui, &row.chat.name, row.chat.is_group, 46.0);
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 56.0);
                    ui.label(
                        RichText::new(&row.chat.name)
                            .size(15.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    );
                    let preview = if row.preview.is_empty() {
                        "Click to chat".to_string()
                    } else {
                        row.preview.clone()
                    };
                    ui.add(
                        egui::Label::new(RichText::new(preview).size(13.0).color(theme.muted)).truncate(),
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.vertical(|ui| {
                        if row.ts > 0 {
                            ui.label(RichText::new(rel_time(row.ts, now)).size(11.0).color(theme.muted));
                        }
                        if row.unread > 0 {
                            unread_badge(ui, row.unread);
                        }
                    });
                });
            });
        });

        if resp.clicked() {
            open_chat(app, row.chat.clone());
        }
        if resp.secondary_clicked() || resp.long_touched() {
            app.state.to_delete = Some(row.chat.clone());
        }
        ui.add_space(2.0);
    }

    delete_confirm(app, ui, theme);
}

/// Desktop toolbar at the top of the conversation-list pane: a "Messages" title
/// and New-chat / New-group icon buttons (replaces the mobile floating FABs).
fn list_toolbar(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.add_space(2.0);
        ui.label(
            RichText::new("Messages")
                .size(17.0)
                .family(icons::semibold())
                .color(theme.ink),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if icon_button(ui, theme, icons::PERSON_ADD, 19.0, theme.gold_ink)
                .on_hover_text("New chat")
                .clicked()
            {
                app.state.modal = Some(Modal::AddContact);
            }
            if icon_button(ui, theme, icons::GROUP_ADD, 19.0, theme.muted)
                .on_hover_text("New group")
                .clicked()
            {
                app.state.modal = Some(Modal::NewGroup);
            }
        });
    });
    ui.add_space(6.0);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme.glass_border),
    );
    ui.add_space(8.0);
}

fn delete_confirm(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let Some(chat) = app.state.to_delete.clone() else { return };
    let ctx = ui.ctx().clone();
    // Dim backdrop that dismisses on a click outside the sheet.
    let screen = ctx.screen_rect();
    let backdrop = egui::Area::new(egui::Id::new("delete-chat-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(&ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(if theme.light { 120 } else { 160 }));
            resp
        });
    if backdrop.inner.clicked() {
        app.state.to_delete = None;
        app.state.block_when_deleting = false;
        return;
    }
    let blocking = app.state.block_when_deleting;

    egui::Window::new("delete-chat")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(&ctx)))
        .frame(theme.sheet())
        .show(&ctx, |ui| {
            ui.set_max_width(320.0);
            theme.sheet_handle(ui);
            let title = if blocking {
                "Block & remove?"
            } else if chat.is_group {
                "Leave & delete group?"
            } else {
                "Delete conversation?"
            };
            ui.label(
                RichText::new(title)
                    .size(20.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
            ui.add_space(6.0);
            let sub = if blocking {
                format!("{} will be blocked and this conversation removed.", chat.name)
            } else {
                chat.name.clone()
            };
            ui.label(RichText::new(sub).size(13.0).color(theme.muted));
            ui.add_space(18.0);
            ui.horizontal(|ui| {
                if super::outline_button(ui, theme, false, "Cancel").clicked() {
                    app.state.to_delete = None;
                    app.state.block_when_deleting = false;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let label = if blocking { "Block" } else { "Delete" };
                    if super::push_button(ui, false, label, LIKE, LIKE.gamma_multiply(1.1), Color32::WHITE)
                        .clicked()
                    {
                        if blocking {
                            // Block the did + delete the conversation (handles
                            // closing the open chat + reload internally).
                            app.block_and_remove(&chat);
                        } else {
                            app.delete_chat(&chat);
                            if app.state.open_chat.as_ref().map(|c| c.id == chat.id).unwrap_or(false) {
                                app.state.open_chat = None;
                                app.state.convo.clear();
                            }
                            app.load_chats();
                        }
                        app.state.to_delete = None;
                        app.state.block_when_deleting = false;
                    }
                });
            });
        });
}

fn open_chat(app: &mut App, chat: OpenChat) {
    if !chat.is_group {
        let did = chat.id.clone();
        app.engine.call(
            &app.ev_tx,
            move || async move {
                hey_mobile_runtime::social::chat_mark_read(&did).await;
            },
            |_| UiEvent::Toast(String::new()),
        );
    }
    app.state.convo.clear();
    app.state.chat_search = None;
    app.state.react_target = None;
    app.load_convo(&chat);
    app.load_msg_reactions(&chat);
    app.state.open_chat = Some(chat);
}

// ── conversation ────────────────────────────────────────────────────────────

fn conversation(app: &mut App, ui: &mut egui::Ui, theme: &Theme, chat: &OpenChat) {
    // Header: back, avatar+name (or search field when open), search toggle.
    ui.horizontal(|ui| {
        if icon_button(ui, theme, icons::ARROW_BACK, 20.0, theme.ink).clicked() {
            app.state.open_chat = None;
            app.state.convo.clear();
            app.state.chat_search = None;
        }
        ui.add_space(4.0);
        if app.state.chat_search.is_some() {
            let mut q = app.state.chat_search.clone().unwrap_or_default();
            let resp = super::field_w(ui, theme, &mut q, "Search messages…", ui.available_width() - 48.0);
            if resp.changed() {
                app.state.chat_search = Some(q);
            }
            if icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                app.state.chat_search = None;
            }
        } else {
            // Header avatar + name → tap opens the ChatInfo sheet (DMs only; a group
            // has no single recipient, matching Android). The avatar + name share one
            // clickable group so the whole "who am I talking to" target is tappable.
            let mut open_info = false;
            let head = ui.horizontal(|ui| {
                gradient_avatar(ui, &chat.name, chat.is_group, 34.0);
                ui.add_space(10.0);
                ui.label(
                    RichText::new(&chat.name)
                        .size(17.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
            });
            if !chat.is_group {
                let resp = head.response.interact(Sense::click()).on_hover_text("Chat info");
                if resp.clicked() {
                    open_info = true;
                }
            }
            if open_info {
                app.state.modal = Some(Modal::ChatInfo(chat.clone()));
            }
            let mut open_tip = false;
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, theme, icons::SEARCH, 18.0, theme.muted).clicked() {
                    app.state.chat_search = Some(String::new());
                }
                // Send crypto (tip) by identity — DMs only (a group has no single
                // recipient). Opens the same Tip sheet as a feed post.
                if !chat.is_group {
                    ui.add_space(2.0);
                    if icon_button(ui, theme, icons::PAID, 19.0, theme.gold_ink)
                        .on_hover_text("Send crypto")
                        .clicked()
                    {
                        open_tip = true;
                    }
                }
            });
            if open_tip {
                let (did, name) = (chat.id.clone(), chat.name.clone());
                app.open_tip(&did, &name);
            }
        }
    });
    ui.add_space(8.0);

    // Filter by the live search query (text contains, case-insensitive).
    let query = app
        .state
        .chat_search
        .clone()
        .filter(|q| !q.trim().is_empty())
        .map(|q| q.to_lowercase());
    let convo = app.state.convo.clone();
    let now = now_ms();

    // Pin the composer at the bottom of the pane; messages scroll above it. The
    // composer grows when the staged-attachment tray (78px) or the transfer bar
    // (24px) is showing, so reserve that height to keep messages from sliding under.
    let extra = if app.state.sending {
        24.0
    } else if !app.state.staged.is_empty() {
        78.0
    } else {
        0.0
    };
    let composer_h = 68.0 + extra;
    let msgs_h = (ui.available_height() - composer_h).max(80.0);
    egui::ScrollArea::vertical()
        .id_source("convo-scroll")
        .auto_shrink([false, false])
        .max_height(msgs_h)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for m in &convo {
                let text = m.get("text").and_then(Value::as_str).unwrap_or("");
                if let Some(q) = &query {
                    if !text.to_lowercase().contains(q) {
                        continue;
                    }
                }
                bubble(app, ui, theme, chat, m, now);
                ui.add_space(2.0);
            }
        });

    ui.add_space(8.0);
    compose_row(app, ui, theme, chat);
    emoji_picker(app, ui, theme, chat);
    edit_dialog(app, ui, theme, chat);
}

fn compose_row(app: &mut App, ui: &mut egui::Ui, theme: &Theme, chat: &OpenChat) {
    // Staged-attachment tray (review/remove before send) + a live transfer bar,
    // both above the input — the desktop parity for Android's staged composer.
    staged_tray(app, ui, theme);
    transfer_bar(app, ui, theme);

    let sending = app.state.sending;
    let has_staged = !app.state.staged.is_empty();
    ui.horizontal(|ui| {
        // Attach now STAGES (multi-pick) rather than sending immediately. Disabled
        // while a batch is in flight, like Android's attach button.
        let attach_col = if sending { theme.faint } else { theme.muted };
        let attach = icon_button(ui, theme, icons::ATTACH_FILE, 20.0, attach_col)
            .on_hover_text("Attach files");
        if attach.clicked() && !sending {
            app.pick_attachments();
        }
        ui.add_space(2.0);
        // Text input doubles as the caption when the tray is non-empty. Framed
        // (radius 12 from apply) with an explicit gold focus ring.
        let fw = (ui.available_width() - 52.0).max(80.0);
        let hint = if has_staged { "Add a caption…" } else { "Message…" };
        let resp = super::field_w(ui, theme, &mut app.state.chat_draft, hint, fw);
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        // Send: a flat gold 44px capsule with the SEND glyph in NAVY (spring press).
        // Enabled when there's text OR staged items; greyed while a batch sends.
        let can_send = !sending && (has_staged || !app.state.chat_draft.trim().is_empty());
        let (srect, send) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), Sense::click());
        let down = send.is_pointer_button_down_on() && can_send;
        let press = ui.ctx().animate_bool_with_time(send.id, down, 0.07);
        let r = egui::Rect::from_center_size(srect.center(), srect.size() * (1.0 - 0.04 * press));
        let fill = if !can_send {
            GOLD.gamma_multiply(0.55)
        } else if send.hovered() && !down {
            GOLD_BRIGHT
        } else {
            GOLD
        };
        ui.painter().rect_filled(r, 12.0, fill);
        ui.painter()
            .text(r.center(), Align2::CENTER_CENTER, icons::SEND, FontId::proportional(20.0), NAVY);
        if (enter || send.clicked()) && can_send {
            if has_staged {
                // Send the staged batch; the text field rides as the caption.
                let caption = std::mem::take(&mut app.state.chat_draft);
                app.send_staged(chat, caption);
            } else {
                send_msg(app, chat);
            }
            resp.request_focus();
        }
    });
}

/// Horizontal tray of staged attachments above the composer input — a 64px tile per
/// item (image thumbnail or a file glyph) with a small ✕ to remove. Hidden while a
/// batch is sending (the transfer bar takes over). Mirrors Android's staged LazyRow.
fn staged_tray(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    if app.state.staged.is_empty() || app.state.sending {
        return;
    }
    let mut remove: Option<usize> = None;
    egui::ScrollArea::horizontal()
        .id_source("staged-tray")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                // Snapshot the items we need (bytes ref for the thumbnail) without
                // holding a borrow across the per-tile mutable texture upload.
                let items: Vec<(usize, bool, Vec<u8>, String)> = app
                    .state
                    .staged
                    .iter()
                    .enumerate()
                    .map(|(i, s)| (i, s.mime.starts_with("image/"), s.bytes.clone(), s.name.clone()))
                    .collect();
                for (i, is_image, bytes, name) in items {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 64.0), Sense::hover());
                    // Tile background.
                    ui.painter().rect_filled(rect, 10.0, theme.glass_fill);
                    if is_image {
                        let key = format!("staged:{}:{}", name, bytes.len());
                        if let Some(tex) = att_texture(app, ui.ctx(), &key, &bytes) {
                            let img = egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                                .max_size(rect.size())
                                .rounding(10.0);
                            img.paint_at(ui, rect);
                        } else {
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                icons::DESCRIPTION,
                                FontId::proportional(26.0),
                                theme.gold_ink,
                            );
                        }
                    } else {
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            icons::DESCRIPTION,
                            FontId::proportional(26.0),
                            theme.gold_ink,
                        );
                    }
                    // ✕ remove chip, top-right.
                    let xr = egui::Rect::from_min_size(
                        egui::pos2(rect.right() - 20.0, rect.top() + 2.0),
                        egui::vec2(18.0, 18.0),
                    );
                    ui.painter().circle_filled(xr.center(), 9.0, Color32::from_black_alpha(0xCC));
                    ui.painter().text(
                        xr.center(),
                        Align2::CENTER_CENTER,
                        icons::CLOSE,
                        FontId::proportional(12.0),
                        Color32::WHITE,
                    );
                    if ui
                        .interact(xr, egui::Id::new(("staged-x", i, &name)), Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        remove = Some(i);
                    }
                    ui.add_space(8.0);
                }
            });
        });
    if let Some(i) = remove {
        if i < app.state.staged.len() {
            app.state.staged.remove(i);
        }
    }
    ui.add_space(6.0);
}

/// Thin "Sending d/t…" label + a determinate progress bar, shown only while a staged
/// batch is in flight. Desktop parity for Android's transfer bar.
fn transfer_bar(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    if !app.state.sending {
        return;
    }
    let (done, total) = (app.state.send_done, app.state.send_total);
    let label = if total > 0 {
        let unit = if total == 1 { "item" } else { "items" };
        format!("Sending {}/{} {unit}…", done.min(total), total)
    } else {
        "Sending…".to_string()
    };
    ui.label(RichText::new(label).size(11.0).color(theme.muted));
    ui.add_space(3.0);
    let frac = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 3.0), Sense::hover());
    ui.painter().rect_filled(rect, 2.0, theme.glass_fill);
    if frac > 0.0 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * frac, rect.height()));
        ui.painter().rect_filled(fill, 2.0, GOLD);
    }
    ui.add_space(6.0);
}

/// Replicates the Android send: DM vs group, then reload the conversation.
fn send_msg(app: &mut App, chat: &OpenChat) {
    let text = std::mem::take(&mut app.state.chat_draft);
    let id = chat.id.clone();
    let is_group = chat.is_group;
    let reload = chat.clone();
    app.engine.call(
        &app.ev_tx,
        move || async move {
            if is_group {
                hey_mobile_runtime::social::chat_send_group(&id, &text).await
            } else {
                hey_mobile_runtime::social::chat_send(&id, &text).await
            }
        },
        |r| match r {
            Ok(_) => UiEvent::Toast(String::new()),
            Err(e) => UiEvent::Error(e),
        },
    );
    app.load_convo(&reload);
}

// ── bubble ──────────────────────────────────────────────────────────────────

fn bubble(app: &mut App, ui: &mut egui::Ui, theme: &Theme, chat: &OpenChat, m: &Value, now: i64) {
    let id = m.get("id").and_then(Value::as_str).unwrap_or("").to_string();
    let text = m.get("text").and_then(Value::as_str).unwrap_or("");
    let mine = m.get("mine").and_then(Value::as_bool).unwrap_or(false);
    let sender = m.get("sender_name").and_then(Value::as_str).unwrap_or("");
    let ts = m.get("ts").and_then(Value::as_i64).unwrap_or(0);
    let atts = m.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default();

    // Reactions for this message (chat_id -> flat list grouped by message_id).
    let grouped = grouped_reactions(app, &chat.id, &id);

    let layout = if mine {
        Layout::right_to_left(Align::Min)
    } else {
        Layout::left_to_right(Align::Min)
    };
    ui.with_layout(layout, |ui| {
        ui.vertical(|ui| {
            ui.with_layout(layout, |ui| {
                let tight = !atts.is_empty() && text.is_empty();
                let pad: f32 = if tight { 6.0 } else { 12.0 };
                let vpad: f32 = if tight { 6.0 } else { 8.0 };
                let fill = if mine { theme.bubble_me } else { theme.bubble_in };
                // The warm gold tint already signals "me" — no border on bubble_me.
                // Incoming bubbles get a hairline only in light mode (none in dark).
                let stroke = if !mine && theme.light {
                    Stroke::new(1.0, theme.glass_border)
                } else {
                    Stroke::NONE
                };
                let frame = egui::Frame::none()
                    .fill(fill)
                    .stroke(stroke)
                    .rounding(18.0)
                    .inner_margin(Margin::symmetric(pad.max(10.0), vpad.max(7.0)));
                let resp = frame
                    .show(ui, |ui| {
                        ui.set_max_width(300.0);
                        ui.vertical(|ui| {
                            if chat.is_group && !mine && !sender.is_empty() {
                                ui.label(
                                    RichText::new(sender)
                                        .size(11.0)
                                        .family(icons::semibold())
                                        .color(super::avatar_palette(sender).0),
                                );
                            }
                            for att in &atts {
                                attachment_view(app, ui, theme, att, mine);
                            }
                            if !text.is_empty() {
                                if !atts.is_empty() {
                                    ui.add_space(6.0);
                                }
                                ui.label(RichText::new(text).size(15.0).color(theme.ink));
                            }
                        });
                    })
                    .response
                    .interact(Sense::click());
                if !id.is_empty() {
                    if mine {
                        // Own message: right-click / long-press → Edit / Delete / React,
                        // matching Android's mine-vs-received action split.
                        resp.context_menu(|ui| {
                            if ui.button("Edit").clicked() {
                                app.state.edit_target = Some(id.clone());
                                app.state.edit_draft = text.to_string();
                                ui.close_menu();
                            }
                            if ui.button("Delete").clicked() {
                                app.delete_message(chat, id.clone());
                                ui.close_menu();
                            }
                            if ui.button("React").clicked() {
                                app.state.react_target = Some(id.clone());
                                ui.close_menu();
                            }
                        });
                    } else if resp.secondary_clicked() || resp.long_touched() {
                        app.state.react_target = Some(id.clone());
                    }
                }
            });

            // Reaction chips under the bubble (recessed pill; tap to toggle yours).
            if !grouped.is_empty() {
                ui.add_space(3.0);
                ui.with_layout(layout, |ui| {
                    for (emoji, count) in &grouped {
                        let chip = egui::Frame::none()
                            .fill(theme.hover)
                            .stroke(Stroke::new(1.0, theme.glass_border))
                            .rounding(999.0)
                            .inner_margin(Margin::symmetric(8.0, 3.0))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("{emoji} {count}"))
                                        .size(12.0)
                                        .family(icons::medium())
                                        .color(theme.ink),
                                );
                            })
                            .response
                            .interact(Sense::click());
                        if chip.clicked() && !id.is_empty() {
                            app.react_message(chat, id.clone(), emoji.clone());
                        }
                        ui.add_space(4.0);
                    }
                });
            }

            if ts > 0 {
                ui.add_space(1.0);
                ui.with_layout(layout, |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(rel_time(ts, now)).size(10.0).color(theme.muted));
                });
            }
        });
    });
}

/// Reactions for `msg_id` within `chat_id`, grouped to (emoji, count), order-stable.
fn grouped_reactions(app: &App, chat_id: &str, msg_id: &str) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    let Some(list) = app.state.msg_reactions.get(chat_id).and_then(Value::as_array) else {
        return out;
    };
    for r in list {
        if r.get("message_id").and_then(Value::as_str) != Some(msg_id) {
            continue;
        }
        let emoji = r.get("emoji").and_then(Value::as_str).unwrap_or("").to_string();
        if emoji.is_empty() {
            continue;
        }
        if let Some(slot) = out.iter_mut().find(|(e, _)| e == &emoji) {
            slot.1 += 1;
        } else {
            out.push((emoji, 1));
        }
    }
    out
}

// ── attachments ──────────────────────────────────────────────────────────────

/// Render one attachment. Images: fetch (keyed by the raw attachment JSON),
/// decode + upload on the UI thread, then draw inline (tap → full-screen zoom).
/// Non-images: a tappable file row (icon + name + human size).
fn attachment_view(app: &mut App, ui: &mut egui::Ui, theme: &Theme, att: &Value, mine: bool) {
    let key = att.to_string();
    let name = att.get("name").and_then(Value::as_str).unwrap_or("file").to_string();
    let mime = att.get("mime").and_then(Value::as_str).unwrap_or("");
    let size = att.get("size").and_then(Value::as_u64).unwrap_or(0);
    let is_image = mime.starts_with("image/");
    let is_video = mime.starts_with("video/");

    if is_image {
        // Kick off the fetch on first sight.
        if !app.state.attachments.contains_key(&key) && !app.state.att_loading.contains(&key) {
            app.state.att_loading.insert(key.clone());
            app.fetch_attachment(key.clone(), key.clone());
        }
        if let Some(bytes) = app.state.attachments.get(&key).cloned() {
            if bytes.is_empty() {
                attachment_failed(app, ui, theme, &key, mine);
                return;
            }
            if let Some(tex) = att_texture(app, ui.ctx(), &key, &bytes) {
                // Inline image → click opens the full-screen viewer (with Save), the
                // desktop parity for Android's FullImageViewer. The decrypted bytes are
                // already in `app.state.attachments[key]`; the viewer reuses them (no
                // second fetch) and decodes via the same `att_texture` path.
                let resp = ui
                    .add(
                        egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                            .max_width(240.0)
                            .max_height(300.0)
                            .rounding(12.0)
                            .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                    app.state.att_viewer = Some(key.clone());
                }
            } else {
                attachment_failed(app, ui, theme, &key, mine);
            }
        } else {
            // still loading
            let (rect, _) = ui.allocate_exact_size(egui::vec2(200.0, 130.0), Sense::hover());
            ui.painter().rect_filled(rect, 12.0, theme.glass_fill);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "…",
                FontId::proportional(24.0),
                theme.gold_ink,
            );
        }
    } else {
        let fill = if mine {
            Color32::from_black_alpha(0x22)
        } else {
            theme.glass_fill
        };
        let glyph = if is_video { icons::PLAY_ARROW } else { icons::DESCRIPTION };
        let label_col = if mine { NAVY } else { theme.ink };
        let sub_col = if mine { NAVY.gamma_multiply(0.7) } else { theme.muted };
        let icon_col = if mine { NAVY } else { theme.gold_ink };
        let resp = egui::Frame::none()
            .fill(fill)
            .rounding(12.0)
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(glyph).size(18.0).color(icon_col));
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        let nm = if name.is_empty() { "file".to_string() } else { name.clone() };
                        ui.label(RichText::new(nm).size(14.0).color(label_col));
                        ui.label(RichText::new(crate::util::human_size(size)).size(11.0).color(sub_col));
                    });
                });
            })
            .response
            .interact(Sense::click());
        if resp.clicked() {
            // Trigger a fetch so the file lands in cache (best-effort "open").
            if !app.state.attachments.contains_key(&key) && !app.state.att_loading.contains(&key) {
                app.state.att_loading.insert(key.clone());
                app.fetch_attachment(key.clone(), key);
            }
        }
    }
}

/// Empty-bytes / decode-fail placeholder with a tap-to-retry.
fn attachment_failed(app: &mut App, ui: &mut egui::Ui, theme: &Theme, key: &str, mine: bool) {
    let resp = egui::Frame::none()
        .fill(theme.glass_fill)
        .rounding(12.0)
        .inner_margin(Margin::same(16.0))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(200.0, 110.0));
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(icons::REFRESH).size(26.0).color(theme.gold_ink));
                ui.add_space(6.0);
                let tc = if mine { NAVY } else { theme.ink };
                ui.label(RichText::new("Click to load photo").size(12.0).color(tc));
            });
        })
        .response
        .interact(Sense::click());
    if resp.clicked() {
        app.state.attachments.remove(key);
        app.state.att_tex.remove(key); // forget any stale/failed texture so it re-uploads
        app.state.att_loading.insert(key.to_string());
        app.fetch_attachment(key.to_string(), key.to_string());
    }
}

/// Decode + upload an attachment image to a texture, memoised in the capped
/// `AppState::att_tex` LRU (was an unbounded `ctx.data` `insert_temp` that leaked a
/// GPU texture per distinct attachment for the whole session). Keyed by the
/// attachment raw JSON; the LRU evicts to `ATT_TEX_CAP`, dropping old handles and
/// freeing their VRAM.
fn att_texture(app: &mut App, ctx: &egui::Context, key: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    if let Some(t) = app.state.att_tex.get(key) {
        return Some(t);
    }
    let dynimg = image::load_from_memory(bytes).ok()?;
    let rgba = dynimg.to_rgba8();
    let (w, h) = rgba.dimensions();
    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], rgba.as_raw());
    let tex = ctx.load_texture(format!("att:{key}"), img, egui::TextureOptions::LINEAR);
    app.state.att_tex.insert(key.to_string(), tex.clone());
    Some(tex)
}

// ── emoji picker ─────────────────────────────────────────────────────────────

fn emoji_picker(app: &mut App, ui: &mut egui::Ui, theme: &Theme, chat: &OpenChat) {
    let Some(msg_id) = app.state.react_target.clone() else { return };
    let ctx = ui.ctx().clone();

    // Dim backdrop that dismisses on a click outside the sheet.
    let screen = ctx.screen_rect();
    let backdrop = egui::Area::new(egui::Id::new("emoji-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(&ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(if theme.light { 120 } else { 160 }));
            resp
        });
    if backdrop.inner.clicked() {
        app.state.react_target = None;
        return;
    }

    // Local iPad slide-up: anim 0→1 over 0.16s (the shared "sheet-anim" tracks the
    // modal state, which this picker isn't, so it drives its own).
    let anim = ctx.animate_bool_with_time(egui::Id::new("emoji-sheet"), true, 0.16);
    egui::Window::new("emoji-picker")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, (1.0 - anim) * 20.0))
        .frame(theme.sheet())
        .show(&ctx, |ui| {
            ui.set_max_width(340.0);
            theme.sheet_handle(ui);
            ui.label(
                RichText::new("React")
                    .size(20.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                for e in EMOJI {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(38.0, 38.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().circle_filled(rect.center(), 19.0, theme.hover);
                    }
                    ui.painter()
                        .text(rect.center(), Align2::CENTER_CENTER, e, FontId::proportional(26.0), theme.ink);
                    if resp.clicked() {
                        app.react_message(chat, msg_id.clone(), e.to_string());
                        app.state.react_target = None;
                    }
                }
            });
        });
}

// ── edit-message dialog (own messages) ───────────────────────────────────────

fn edit_dialog(app: &mut App, ui: &mut egui::Ui, theme: &Theme, chat: &OpenChat) {
    let Some(msg_id) = app.state.edit_target.clone() else { return };
    let ctx = ui.ctx().clone();

    let screen = ctx.screen_rect();
    let backdrop = egui::Area::new(egui::Id::new("edit-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(&ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(if theme.light { 120 } else { 160 }));
            resp
        });
    if backdrop.inner.clicked() {
        app.state.edit_target = None;
        app.state.edit_draft.clear();
        return;
    }

    let anim = ctx.animate_bool_with_time(egui::Id::new("edit-sheet"), true, 0.16);
    let mut save = false;
    let mut cancel = false;
    egui::Window::new("edit-message")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, (1.0 - anim) * 20.0))
        .frame(theme.sheet())
        .show(&ctx, |ui| {
            ui.set_max_width(380.0);
            theme.sheet_handle(ui);
            ui.label(
                RichText::new("Edit message")
                    .size(20.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
            ui.add_space(12.0);
            ui.add(
                egui::TextEdit::multiline(&mut app.state.edit_draft)
                    .desired_width(f32::INFINITY)
                    .desired_rows(3),
            );
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Cancel").color(theme.muted)).clicked() {
                    cancel = true;
                }
                ui.add_space(8.0);
                if ui.button(RichText::new("Save").color(theme.ink)).clicked() {
                    save = true;
                }
            });
        });
    if save {
        let txt = app.state.edit_draft.trim().to_string();
        if !txt.is_empty() {
            app.edit_message(chat, msg_id, txt);
        }
        app.state.edit_target = None;
        app.state.edit_draft.clear();
    } else if cancel {
        app.state.edit_target = None;
        app.state.edit_draft.clear();
    }
}

// ── small shared widgets ─────────────────────────────────────────────────────

/// Clean Gold2→Gold gradient circle (no gloss) with a group glyph or the name's
/// first letter, glyph in NAVY (on-gold ink).
fn gradient_avatar(ui: &mut egui::Ui, name: &str, is_group: bool, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), Sense::hover());
    let c = rect.center();
    super::gradient_circle(ui.painter(), c, size / 2.0, GOLD2, GOLD);
    let glyph = if is_group {
        "#".to_string()
    } else {
        name.chars().next().unwrap_or('?').to_uppercase().to_string()
    };
    ui.painter().text(
        c,
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(size * 0.42, icons::semibold()),
        NAVY,
    );
}

// ── full-screen attachment image viewer (GAP A) ──────────────────────────────

/// Full-screen viewer for an inline chat image, opened by tapping the bubble image
/// (the desktop parity for Android's `FullImageViewer`). Reuses the ALREADY-decoded
/// bytes in `app.state.attachments[key]` (no second fetch) + the same `att_texture`
/// decode path the inline render uses. Dim backdrop, image fit to screen, scroll to
/// zoom, a Save button (writes the bytes via the native dialog / Pictures fallback),
/// and click-outside / Close / Esc to dismiss.
pub fn attachment_viewer(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let Some(key) = app.state.att_viewer.clone() else { return };
    // Suggested filename from the attachment JSON (the key IS the raw attachment).
    let name = serde_json::from_str::<Value>(&key)
        .ok()
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "hey-photo".to_string());

    let scale_id = egui::Id::new(("att-zoom", &key));
    let mut scale: f32 = ctx.memory(|m| m.data.get_temp(scale_id).unwrap_or(1.0_f32));

    let mut close = false;
    let mut do_save = false;
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("att-viewer-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let rect = screen;
            ui.painter().rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 245));
            let bg = ui.allocate_rect(rect, Sense::click());

            if ui.rect_contains_pointer(rect) {
                let dy = ctx.input(|i| i.raw_scroll_delta.y);
                if dy.abs() > 0.0 {
                    scale = (scale * (1.0 + dy * 0.0015)).clamp(1.0, 5.0);
                }
            }

            // Draw the image from the cached decoded bytes + texture.
            let bytes = app.state.attachments.get(&key).cloned();
            let tex = bytes.and_then(|b| att_texture(app, ctx, &key, &b));
            if let Some(tex) = tex {
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
                    Align2::CENTER_CENTER,
                    "…",
                    FontId::proportional(34.0),
                    Color32::WHITE,
                );
            }

            // Top-right: Save + Close (material circles over the dark backdrop).
            let save_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 104.0, rect.top() + 16.0),
                egui::vec2(40.0, 40.0),
            );
            let save = ui.put(
                save_rect,
                egui::Button::new(RichText::new(icons::DOWNLOAD).size(20.0).color(theme.ink))
                    .fill(theme.surface2)
                    .stroke(Stroke::new(1.0, theme.glass_border))
                    .rounding(20.0),
            )
            .on_hover_text("Save");
            let close_rect = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 56.0, rect.top() + 16.0),
                egui::vec2(40.0, 40.0),
            );
            let close_btn = ui.put(
                close_rect,
                egui::Button::new(RichText::new(icons::CLOSE).size(20.0).color(theme.ink))
                    .fill(theme.surface2)
                    .stroke(Stroke::new(1.0, theme.glass_border))
                    .rounding(20.0),
            );

            ui.painter().text(
                egui::pos2(rect.center().x, rect.bottom() - 24.0),
                Align2::CENTER_CENTER,
                "Scroll to zoom",
                FontId::proportional(12.0),
                Color32::from_white_alpha(150),
            );

            if save.clicked() {
                do_save = true;
            }
            if close_btn.clicked() || (bg.clicked() && scale <= 1.001) {
                close = true;
            }
            ctx.memory_mut(|m| m.data.insert_temp(scale_id, scale));
        });

    if do_save {
        if let Some(bytes) = app.state.attachments.get(&key).cloned() {
            if bytes.is_empty() {
                let now = ctx.input(|i| i.time);
                app.toast("Nothing to save yet", now);
            } else {
                app.save_attachment(bytes, name);
            }
        }
    }
    if close {
        app.state.att_viewer = None;
        ctx.memory_mut(|m| m.data.remove::<f32>(scale_id));
    }
}

// ── chat-info sheet (GAP B) ──────────────────────────────────────────────────

/// The ChatInfo sheet (header avatar/name tap) — desktop parity for Android's
/// `ChatInfoSheet`. DM-only (groups have no single recipient). Rows: View profile,
/// Send a gift / tip, Mute notifications (toggle, persisted), Block & remove
/// (confirm → block the did + delete the conversation). Uses the same centered
/// `theme.sheet()` idiom as the emoji picker / profile sheets.
pub fn chat_info_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme, chat: &OpenChat) {
    // Dim backdrop that dismisses on a click outside the sheet.
    let screen = ctx.screen_rect();
    let backdrop = egui::Area::new(egui::Id::new("chatinfo-backdrop"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, Color32::from_black_alpha(if theme.light { 120 } else { 160 }));
            resp
        });
    if backdrop.inner.clicked() {
        app.state.modal = None;
        return;
    }

    let muted = app.state.muted_chats.contains(&chat.id);
    let mut do_view_profile = false;
    let mut do_tip = false;
    let mut toggle_mute = false;
    let mut do_block = false;

    let anim = ctx.animate_bool_with_time(egui::Id::new("chatinfo-sheet"), true, 0.16);
    egui::Window::new("chat-info")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, (1.0 - anim) * 20.0))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_max_width(360.0);
            theme.sheet_handle(ui);

            // Header: avatar + name + an "end-to-end encrypted" reassurance row.
            ui.horizontal(|ui| {
                gradient_avatar(ui, &chat.name, false, 56.0);
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(&chat.name)
                            .size(20.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    );
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icons::LOCK).size(11.0).color(theme.good));
                        ui.add_space(3.0);
                        ui.label(RichText::new("end-to-end encrypted").size(12.0).color(theme.muted));
                    });
                });
            });
            ui.add_space(14.0);

            // View profile
            if info_row(ui, theme, icons::PERSON, "View profile", false) {
                do_view_profile = true;
            }
            // Send a gift / tip
            if info_row(ui, theme, icons::PAID, "Send a gift / tip", false) {
                do_tip = true;
            }
            // Mute notifications (toggle row)
            let mute_glyph = if muted { icons::NOTIFICATIONS_OFF } else { icons::NOTIFICATIONS };
            let mute_resp = egui::Frame::none()
                .fill(Color32::TRANSPARENT)
                .rounding(12.0)
                .inner_margin(Margin::symmetric(12.0, 11.0))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(mute_glyph).size(19.0).color(theme.gold_ink));
                        ui.add_space(12.0);
                        ui.label(RichText::new("Mute notifications").size(15.0).color(theme.ink));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // A compact pill that reads on/off (egui's Switch isn't in the
                            // shared kit; a tinted pill matches the design system).
                            let (on, off) = (GOLD, theme.hover);
                            let pill = egui::Frame::none()
                                .fill(if muted { on } else { off })
                                .stroke(Stroke::new(1.0, theme.glass_border))
                                .rounding(999.0)
                                .inner_margin(Margin::symmetric(10.0, 4.0))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(if muted { "On" } else { "Off" })
                                            .size(12.0)
                                            .family(icons::medium())
                                            .color(if muted { NAVY } else { theme.muted }),
                                    );
                                });
                            let _ = pill;
                        });
                    });
                })
                .response
                .interact(Sense::click());
            if mute_resp.hovered() {
                ui.painter().rect_filled(mute_resp.rect, 12.0, theme.hover);
            }
            if mute_resp.clicked() {
                toggle_mute = true;
            }
            // Block & remove (danger)
            if info_row(ui, theme, icons::BLOCK, "Block & remove", true) {
                do_block = true;
            }
        });

    // Apply the chosen action AFTER the window closes its borrow of `ctx`.
    if do_view_profile {
        let did = chat.id.clone();
        app.state.modal = None;
        app.state.viewed = Some(crate::state::ViewedUser { did: did.clone(), ..Default::default() });
        app.load_user(&did);
    } else if do_tip {
        let (did, name) = (chat.id.clone(), chat.name.clone());
        app.state.modal = None;
        app.open_tip(&did, &name);
    } else if toggle_mute {
        app.set_chat_muted(&chat.id, !muted);
    } else if do_block {
        // Confirm before the destructive block + delete — reuses the delete-confirm
        // dialog, flagged so it blocks the did as well as deleting the conversation.
        app.state.modal = None;
        app.state.to_delete = Some(chat.clone());
        app.state.block_when_deleting = true;
    }
}

/// One tappable ChatInfo row: glyph + label, gold-ink (or LIKE for danger), with a
/// soft hover wash. Returns true when clicked. Mirrors Android's `ChatInfoAction`.
fn info_row(ui: &mut egui::Ui, theme: &Theme, glyph: &str, label: &str, danger: bool) -> bool {
    let col = if danger { LIKE } else { theme.gold_ink };
    let txt = if danger { LIKE } else { theme.ink };
    let resp = egui::Frame::none()
        .fill(Color32::TRANSPARENT)
        .rounding(12.0)
        .inner_margin(Margin::symmetric(12.0, 11.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(glyph).size(19.0).color(col));
                ui.add_space(12.0);
                ui.label(RichText::new(label).size(15.0).color(txt));
            });
        })
        .response
        .interact(Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(resp.rect, 12.0, theme.hover);
    }
    resp.clicked()
}

/// Unified unread count pill: LIKE fill, white SemiBold, min 18px wide.
fn unread_badge(ui: &mut egui::Ui, count: i64) {
    let txt = count.min(99).to_string();
    let w = (16.0 + txt.len() as f32 * 5.0).max(18.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(w, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 9.0, LIKE);
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        txt,
        FontId::new(10.5, icons::semibold()),
        Color32::WHITE,
    );
}
