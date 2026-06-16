//! Activity tab — desktop master-detail port of the Android `NotificationsScreen`.
//!
//! Desktop layout (two panes, full content width):
//!   • LEFT pane (~380px, scrollable): the activity list. The drained post/follow
//!     notifications (`app.state.notifs`, newest first) render as flat rows, then
//!     the follower list (each follower who started following you, with a 38px
//!     gradient avatar, "started following you", and either a muted "Following"
//!     label or an inline gold "Follow back" button). Clicking a follower row
//!     selects it (`app.state.activity_selected`) and populates the detail pane;
//!     it no longer opens a full-screen overlay directly.
//!   • RIGHT pane (fills, scrollable): a DETAIL panel. With a follower selected it
//!     shows that person (72px avatar, short + full DID, follow-back, and a
//!     "View full profile" button that opens the `ViewedUser` overlay). With
//!     nothing selected it shows a friendly "your node" summary — your avatar,
//!     nickname, a live connection line, follower/following counts, and a gold
//!     "Share invite link" primary button (copies `friend_link` + a toast).
//!
//! Behaviour parity with the original column:
//!   • If `activity_loaded && followers.is_empty()` (and no notifs) → the left
//!     pane shows the empty state; the right pane still shows the node summary.
//!   • Clicking a notification row that carries a DID opens that actor's profile
//!     overlay (`open_user`), matching the old surfacing.
//!   • "Follow back" calls `app.follow_back(did)` then reloads activity.
//!   • The foundation auto-polls followers/following every 3s while this tab is
//!     open (see `App::poll`), so no manual refresh is wired here.

use std::collections::HashSet;

use egui::{Align, Layout, RichText, Stroke};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{AppState, ViewedUser};
use crate::theme::Theme;

use super::{avatar, chip, empty_state, list_row, outline_button, pill_button, primary_button};

/// Fixed width of the left activity-list pane.
const LEFT_W: f32 = 380.0;

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let avail_h = ui.available_height();

    ui.horizontal_top(|ui| {
        // ── LEFT: the activity list ──
        ui.allocate_ui_with_layout(
            egui::vec2(LEFT_W, avail_h),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(avail_h);
                left_pane(app, ui, theme);
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

        // ── RIGHT: detail panel ──
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), avail_h),
            Layout::top_down(Align::Min),
            |ui| {
                ui.set_min_height(avail_h);
                egui::ScrollArea::vertical()
                    .id_source("activity-right")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        right_pane(app, ui, theme);
                    });
            },
        );
    });
}

// ── left pane: notifications + followers, as flat rows ──────────────────────────

fn left_pane(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // Header.
    ui.add_space(2.0);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.label(
            RichText::new("Activity")
                .size(17.0)
                .family(icons::semibold())
                .color(theme.ink),
        );
    });
    ui.add_space(6.0);
    ui.painter().hline(
        ui.max_rect().x_range(),
        ui.cursor().top(),
        Stroke::new(1.0, theme.glass_border),
    );
    ui.add_space(2.0);

    // Snapshot the notifications (VecDeque, newest at the back → reverse for
    // newest-first) and the follower list so we can freely mutate app.state /
    // dispatch inside the loops.
    let notifs: Vec<Value> = app.state.notifs.iter().rev().cloned().collect();
    let followers = app.state.followers.clone();

    // Set of DIDs we already follow → "Following" vs "Follow back".
    let following: HashSet<String> = app
        .state
        .following
        .iter()
        .filter_map(|f| f.get("did").and_then(Value::as_str))
        .map(str::to_string)
        .collect();

    egui::ScrollArea::vertical()
        .id_source("activity-left")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());

            // Empty state: only when we've loaded and there is nothing at all.
            if app.state.activity_loaded && followers.is_empty() && notifs.is_empty() {
                empty_state(
                    ui,
                    theme,
                    icons::NOTIFICATIONS,
                    "No activity yet",
                    "Share your invite (right) so people can follow you.",
                );
                return;
            }

            // ── Notifications section ──
            if !notifs.is_empty() {
                section_label(ui, theme, "Notifications");
                for n in &notifs {
                    notif_row(app, ui, theme, n);
                }
                ui.add_space(4.0);
                section_label(ui, theme, "Followers");
            }

            // ── Follower list (flat rows) ──
            for f in &followers {
                let did = f.get("did").and_then(Value::as_str).unwrap_or("").to_string();
                if did.is_empty() {
                    continue;
                }
                follower_row(app, ui, theme, &did, &following);
            }
        });
}

/// A muted subhead section header inside the left pane (subhead 13 SemiBold muted).
fn section_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new(text)
                .size(13.0)
                .family(icons::semibold())
                .color(theme.muted),
        );
    });
    ui.add_space(2.0);
}

/// One follower as a flat `list_row`: avatar + short DID + "started following you",
/// an inline "Follow back" / "Following" trailing control. Clicking the row body
/// selects this person for the detail pane.
fn follower_row(
    app: &mut App,
    ui: &mut egui::Ui,
    theme: &Theme,
    did: &str,
    following: &HashSet<String>,
) {
    let already = following.contains(did);
    let selected = app.state.activity_selected.as_deref() == Some(did);
    let mut do_follow_back = false;

    let resp = list_row(ui, theme, selected, |ui| {
        ui.horizontal(|ui| {
            avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", did, 38.0);
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(AppState::short_did(did))
                        .size(15.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.label(
                    RichText::new("started following you")
                        .size(12.0)
                        .color(theme.muted),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if already {
                    ui.label(RichText::new("Following").size(12.0).color(theme.muted));
                } else if pill_button(ui, theme, "Follow back").clicked() {
                    do_follow_back = true;
                }
            });
        });
    });

    if do_follow_back {
        // Follow back, then refresh followers/following immediately (mirrors the
        // Android `tick++`; the 3s poll also catches up).
        app.follow_back(did);
        app.load_activity();
    } else if resp.clicked() {
        // Tapping the row body selects this follower for the detail pane.
        app.state.activity_selected = Some(did.to_string());
    }
}

/// A single notification row (post / follow), flat-styled. Clickable when it
/// carries a DID: taps open the actor's profile overlay (parity with the original).
fn notif_row(app: &mut App, ui: &mut egui::Ui, theme: &Theme, n: &Value) {
    let kind = n.get("kind").and_then(Value::as_str).unwrap_or("");
    let title = n.get("title").and_then(Value::as_str).unwrap_or("");
    let body = n.get("body").and_then(Value::as_str).unwrap_or("");
    let did = n.get("did").and_then(Value::as_str).unwrap_or("").to_string();

    // Glyph hints the notification kind (Material icons, matching Android).
    let glyph = match kind {
        "post" => icons::PHOTO_CAMERA,
        "follow" => icons::PERSON,
        // A mention ("mentioned you in a post") gets the chat-bubble glyph,
        // matching the Android "mentioned you" treatment.
        "mention" => icons::CHAT_BUBBLE_OUTLINE,
        _ => icons::NOTIFICATIONS,
    };
    // Fall back to a short DID when the title is blank (e.g. unnamed actor).
    let heading = if title.is_empty() && !did.is_empty() {
        AppState::short_did(&did)
    } else {
        title.to_string()
    };

    let resp = list_row(ui, theme, false, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(glyph).size(20.0).color(theme.gold_ink));
            ui.add_space(10.0);
            ui.vertical(|ui| {
                if !heading.is_empty() {
                    ui.label(
                        RichText::new(heading)
                            .size(15.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    );
                }
                if !body.is_empty() {
                    ui.label(RichText::new(body).size(12.0).color(theme.muted));
                }
            });
        });
    });

    if resp.clicked() && !did.is_empty() {
        open_user(app, &did);
    }
}

// ── right pane: detail panel ────────────────────────────────────────────────────

fn right_pane(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    if let Some(did) = app.state.activity_selected.clone() {
        follower_detail(app, ui, theme, &did);
    } else {
        node_summary(app, ui, theme);
    }
}

/// Detail for the selected follower: large avatar, identity, follow-back, and a
/// hand-off to the full-screen profile overlay.
fn follower_detail(app: &mut App, ui: &mut egui::Ui, theme: &Theme, did: &str) {
    let already = app
        .state
        .following
        .iter()
        .filter_map(|f| f.get("did").and_then(Value::as_str))
        .any(|d| d == did);

    ui.add_space(24.0);
    ui.vertical_centered(|ui| {
        avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", did, 72.0);
        ui.add_space(10.0);
        ui.label(
            RichText::new(AppState::short_did(did))
                .size(22.0)
                .family(icons::semibold())
                .color(theme.ink),
        );
        ui.add_space(2.0);
        ui.label(RichText::new(did).size(11.0).color(theme.muted));
        ui.add_space(8.0);
        ui.label(
            RichText::new("started following you")
                .size(13.0)
                .color(theme.muted),
        );
        ui.add_space(18.0);

        // Follow back / Following.
        if already {
            chip(ui, theme, "Following", theme.good);
        } else if primary_button(ui, false, "Follow back").clicked() {
            app.follow_back(did);
            app.load_activity();
        }
        ui.add_space(10.0);

        // View full profile → open the shared full-screen overlay.
        if outline_button(
            ui,
            theme,
            false,
            &format!("{}  View full profile", icons::ACCOUNT_CIRCLE),
        )
        .clicked()
        {
            open_user(app, did);
        }

        ui.add_space(14.0);
        // Copy this DID — small utility, neutral.
        if ui
            .add(
                egui::Button::new(
                    RichText::new(format!("{}  Copy DID", icons::LINK))
                        .size(12.0)
                        .color(theme.muted),
                )
                .frame(false),
            )
            .clicked()
        {
            let d = did.to_string();
            ui.output_mut(|o| o.copied_text = d);
            let now = ui.input(|i| i.time);
            app.toast("DID copied", now);
        }
    });
}

/// Default detail: a friendly "your node" summary — your identity, a live
/// connection line, follower/following counts, and the invite-link primary action.
fn node_summary(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let me = app.state.me_did.clone();
    let nickname = app
        .state
        .profile
        .get("nickname")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    ui.add_space(28.0);
    ui.vertical_centered(|ui| {
        avatar(&mut app.media, &app.engine, &app.ev_tx, ui, "", &me, 64.0);
        ui.add_space(10.0);
        ui.label(
            RichText::new(nickname.unwrap_or_else(|| "Your node".to_string()))
                .size(22.0)
                .family(icons::semibold())
                .color(theme.ink),
        );
        if !me.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(AppState::short_did(&me)).size(12.0).color(theme.muted));
        }
        ui.add_space(14.0);

        // Live connection line — a small status dot + peer/direct summary.
        ui.horizontal(|ui| {
            let dot = if app.state.online { theme.good } else { theme.faint };
            let (r, _) = ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
            ui.painter().circle_filled(r.center(), 4.5, dot);
            ui.add_space(2.0);
            let line = if app.state.online {
                format!("Live · {} peers", app.state.peers)
            } else {
                "Connecting…".to_string()
            };
            ui.label(
                RichText::new(line)
                    .size(13.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
        });
        if app.state.online {
            ui.add_space(3.0);
            ui.label(
                RichText::new(if app.state.direct {
                    "Direct peer-to-peer"
                } else {
                    "Relay-assisted · encrypted"
                })
                .size(11.0)
                .color(theme.muted),
            );
        }
        ui.add_space(18.0);

        // Follower / following counts.
        ui.horizontal(|ui| {
            mini_stat(ui, theme, app.state.followers.len(), "followers");
            ui.add_space(8.0);
            mini_stat(ui, theme, app.state.following.len(), "following");
        });
        ui.add_space(20.0);

        // Invite — the one gold primary action on this pane.
        if primary_button(ui, false, "Share invite link").clicked()
            && !app.state.friend_link.is_empty()
        {
            let l = app.state.friend_link.clone();
            ui.output_mut(|o| o.copied_text = l);
            let now = ui.input(|i| i.time);
            app.toast("Invite link copied", now);
        }
        ui.add_space(8.0);
        ui.label(
            RichText::new("Pick a follower on the left to view them.")
                .size(12.0)
                .color(theme.muted),
        );
    });
}

/// A compact count + label stat block (flat material card).
fn mini_stat(ui: &mut egui::Ui, theme: &Theme, count: usize, label: &str) {
    theme.glass(12.0).show(ui, |ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(count.to_string())
                    .size(20.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
            ui.label(RichText::new(label).size(11.0).color(theme.muted));
        });
    });
}

/// Open the full-screen "view another user" overlay for `did` and kick off its load.
fn open_user(app: &mut App, did: &str) {
    app.state.viewed = Some(ViewedUser {
        did: did.to_string(),
        ..Default::default()
    });
    app.load_user(did);
}
