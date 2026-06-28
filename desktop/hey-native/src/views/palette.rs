//! The Command Palette (Ctrl/Cmd+K) — Linear/Superhuman "run anything" surface.
//!
//! A centered-top floating `Area` over the existing modal dim scrim: an auto-focused
//! frameless search field with a gold caret, a subsequence FUZZY scorer over the
//! command set, and a scored result list whose selection is a 2px gold LEFT EDGE
//! that GLIDES between rows (animate_value on the selected index — the signature
//! micro-interaction). Keyboard: ↑/↓ (or Ctrl/Cmd+N/P) move, Enter runs, Esc closes
//! (handled as the topmost peel in the app.rs Esc ladder). Each row shows its
//! keyboard shortcut on the right.
//!
//! The palette owns NO dispatch: `ui()` returns the chosen `PaletteAction` (if any)
//! to `App`, which applies it against `&mut self` — so the same view-switch/modal
//! state the global keymap drives is the state the palette drives. Commands whose
//! actions don't exist yet are STUBBED with a clear label (they toast "coming soon").

use egui::{Align2, FontId, Key, RichText, Sense, Stroke};

use crate::icons;
use crate::state::{AppState, Tab};
use crate::theme::{Theme, GOLD};

/// What the palette resolved to when the user pressed Enter / clicked a row. `App`
/// pattern-matches this and applies it (the palette never holds `&mut App`).
#[derive(Clone)]
pub enum PaletteAction {
    /// Switch to a section (Chat/Feed/Wallet/Verse/Calls/You).
    Go(Tab),
    /// Toggle the Light/Dark theme.
    ToggleTheme,
    /// Open the Settings sheet.
    Settings,
    /// Open the post composer (New post).
    NewPost,
    /// Open the Connection sheet.
    Connection,
    /// Open the "?" cheat-sheet overlay.
    CheatSheet,
    /// Start a voice call with the currently-open chat contact.
    StartCall { video: bool },
    /// A command whose real action isn't wired yet — toast its label.
    Stub(String),
}

/// One palette command: a glyph, the human label, a category chip, an optional
/// shortcut hint (drawn right-aligned), the action, and whether it's currently
/// available (unavailable commands are hidden, e.g. "Start call" with no open chat).
struct Cmd {
    glyph: &'static str,
    label: String,
    category: &'static str,
    shortcut: String,
    action: PaletteAction,
    available: bool,
}

/// Subsequence FUZZY score: every char of `q` (lowercased) must appear IN ORDER in
/// `text` (lowercased). Returns `None` if it doesn't match at all; otherwise a score
/// where lower = better (earlier matches, contiguous runs, and word-boundary hits
/// rank higher). An empty query matches everything with a neutral score so the full
/// list shows in its declared order.
fn fuzzy(q: &str, text: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = text.to_lowercase().chars().collect();
    let needle: Vec<char> = q.to_lowercase().chars().collect();
    let mut hi = 0usize; // index into hay
    let mut score = 0i32;
    let mut last_match: Option<usize> = None;
    for &nc in &needle {
        // Advance `hi` to the next occurrence of `nc`.
        let mut found = None;
        while hi < hay.len() {
            if hay[hi] == nc {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let Some(pos) = found else { return None };
        // Penalise distance from the previous matched char (reward contiguity); a big
        // bonus when the match starts a word (start-of-string or after a space).
        if let Some(prev) = last_match {
            score += (pos - prev) as i32; // gaps cost
        } else {
            score += pos as i32; // how far in the first char is
        }
        let at_boundary = pos == 0 || hay[pos - 1] == ' ' || hay[pos - 1] == ':';
        if at_boundary {
            score -= 4;
        }
        last_match = Some(pos);
        hi = pos + 1;
    }
    Some(score)
}

/// Platform-correct modifier label ("⌘" macOS / "Ctrl+" else) for shortcut hints.
fn cmd_mod() -> &'static str {
    if cfg!(target_os = "macos") { "⌘" } else { "Ctrl+" }
}

/// Build the full command set, marking each available-or-not from `state`. The order
/// here is the empty-query order (most-reached first).
fn commands(state: &AppState) -> Vec<Cmd> {
    let m = cmd_mod();
    let open_chat_name = state.open_chat.as_ref().map(|c| c.name.clone());
    let has_dm = state
        .open_chat
        .as_ref()
        .map(|c| !c.is_group)
        .unwrap_or(false);
    let mut v = vec![
        Cmd {
            glyph: icons::FORUM,
            label: "Go to Chat".into(),
            category: "Jump",
            shortcut: format!("{m}1"),
            action: PaletteAction::Go(Tab::Chat),
            available: true,
        },
        Cmd {
            glyph: icons::DYNAMIC_FEED,
            label: "Go to Feed".into(),
            category: "Jump",
            shortcut: format!("{m}2"),
            action: PaletteAction::Go(Tab::Feed),
            available: true,
        },
        Cmd {
            glyph: icons::ACCOUNT_BALANCE_WALLET,
            label: "Go to Wallet".into(),
            category: "Jump",
            shortcut: format!("{m}3"),
            action: PaletteAction::Go(Tab::Wallet),
            available: true,
        },
        Cmd {
            glyph: icons::PUBLIC,
            label: "Go to Verse".into(),
            category: "Jump",
            shortcut: format!("{m}4"),
            action: PaletteAction::Go(Tab::Verse),
            available: true,
        },
        Cmd {
            glyph: icons::CALL,
            label: "Go to Calls".into(),
            category: "Jump",
            shortcut: format!("{m}5"),
            action: PaletteAction::Go(Tab::Calls),
            available: true,
        },
        Cmd {
            glyph: icons::PERSON,
            label: "Go to You".into(),
            category: "Jump",
            shortcut: format!("{m}6"),
            action: PaletteAction::Go(Tab::Profile),
            available: true,
        },
        Cmd {
            glyph: icons::ADD,
            label: "New post".into(),
            category: "Feed",
            shortcut: format!("{m}N"),
            action: PaletteAction::NewPost,
            available: true,
        },
        Cmd {
            glyph: icons::CALL,
            label: open_chat_name
                .as_deref()
                .map(|n| format!("Start voice call · {n}"))
                .unwrap_or_else(|| "Start voice call".into()),
            category: "Call",
            shortcut: String::new(),
            action: PaletteAction::StartCall { video: false },
            available: has_dm,
        },
        Cmd {
            glyph: icons::VIDEOCAM,
            label: open_chat_name
                .as_deref()
                .map(|n| format!("Start video call · {n}"))
                .unwrap_or_else(|| "Start video call".into()),
            category: "Call",
            shortcut: String::new(),
            action: PaletteAction::StartCall { video: true },
            available: has_dm,
        },
        Cmd {
            glyph: if state.light { icons::VISIBILITY_OFF } else { icons::VISIBILITY },
            label: "Toggle theme".into(),
            category: "Setting",
            shortcut: format!("{m}D"),
            action: PaletteAction::ToggleTheme,
            available: true,
        },
        Cmd {
            glyph: icons::SETTINGS,
            label: "Open Settings".into(),
            category: "Setting",
            shortcut: format!("{m},"),
            action: PaletteAction::Settings,
            available: true,
        },
        Cmd {
            glyph: icons::HUB,
            label: "Connection details".into(),
            category: "Network",
            shortcut: String::new(),
            action: PaletteAction::Connection,
            available: true,
        },
        Cmd {
            glyph: icons::NOTIFICATIONS,
            label: "Keyboard shortcuts".into(),
            category: "Help",
            shortcut: "?".into(),
            action: PaletteAction::CheatSheet,
            available: true,
        },
        // ── stubs (clear label; toast "coming soon" until the real action lands) ──
        Cmd {
            glyph: icons::PERSON_ADD,
            label: "Add contact".into(),
            category: "Chat",
            shortcut: String::new(),
            action: PaletteAction::Stub("Add contact".into()),
            available: true,
        },
        Cmd {
            glyph: icons::SEND,
            label: "Send a message".into(),
            category: "Chat",
            shortcut: String::new(),
            action: PaletteAction::Stub("Send a message".into()),
            available: open_chat_name.is_some(),
        },
        Cmd {
            glyph: icons::ARROW_UPWARD,
            label: "Send funds".into(),
            category: "Wallet",
            shortcut: String::new(),
            action: PaletteAction::Stub("Send funds".into()),
            available: true,
        },
    ];
    v.retain(|c| c.available);
    v
}

/// Draw the palette and read its keyboard. Returns `Some(action)` when the user runs
/// a command (the palette is closed by the caller as it applies the action). Returns
/// `None` otherwise. Closing on Esc is handled by the app.rs Esc ladder (palette is
/// the topmost peel), so this never closes itself except after a run.
pub fn ui(
    state: &mut AppState,
    ctx: &egui::Context,
    theme: &Theme,
) -> Option<PaletteAction> {
    // Take the open-state out so we can mutate query/selected freely, then put it back.
    let Some(mut pal) = state.palette.clone() else { return None };

    // The full filtered+scored list for the CURRENT query.
    let all = commands(state);
    let mut scored: Vec<(i32, usize)> = all
        .iter()
        .enumerate()
        .filter_map(|(i, c)| fuzzy(&pal.query, &c.label).map(|s| (s, i)))
        .collect();
    // Stable: sort by score, ties keep declared order (the enumerate index).
    scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let n = scored.len();
    // Clamp the selection into range (the query may have shrunk the list).
    if n == 0 {
        pal.selected = 0;
    } else if pal.selected >= n {
        pal.selected = n - 1;
    }

    // ── keyboard (read BEFORE the TextEdit consumes it) ──────────────────────────
    // Arrows are ignored by TextEdit; Enter is NOT, so consume it. Ctrl/Cmd+N/P also
    // move (Superhuman/emacs muscle memory) without leaving the home row.
    let mut run = false;
    ctx.input_mut(|i| {
        let cmd_n = i.consume_key(egui::Modifiers::COMMAND, Key::N);
        let cmd_p = i.consume_key(egui::Modifiers::COMMAND, Key::P);
        if (i.key_pressed(Key::ArrowDown) || cmd_n) && n > 0 {
            pal.selected = (pal.selected + 1) % n;
        }
        if (i.key_pressed(Key::ArrowUp) || cmd_p) && n > 0 {
            pal.selected = (pal.selected + n - 1) % n;
        }
        if i.consume_key(egui::Modifiers::NONE, Key::Enter) && n > 0 {
            run = true;
        }
    });

    let mut chosen: Option<PaletteAction> = None;
    let mut click_idx: Option<usize> = None;

    // ── the floating panel: CENTER_TOP +120px, ~560px, over the dim scrim ────────
    egui::Area::new(egui::Id::new("command-palette"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 120.0))
        .show(ctx, |ui| {
            theme.floating(10.0).show(ui, |ui| {
                ui.set_width(560.0);

                // ── auto-focused frameless search field with a gold caret ──────────
                ui.horizontal(|ui| {
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!("{}K", cmd_mod()))
                            .size(13.0)
                            .family(icons::semibold())
                            .color(theme.gold_ink),
                    );
                    ui.add_space(8.0);
                    // Gold caret/cursor on the frameless field.
                    let prev = ui.visuals().clone();
                    ui.visuals_mut().text_cursor.stroke = Stroke::new(2.0, GOLD);
                    ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
                    let field = egui::TextEdit::singleline(&mut pal.query)
                        .frame(false)
                        .hint_text("Search or run a command…")
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(15.0))
                        .show(ui);
                    *ui.visuals_mut() = prev;
                    // Grab focus on the frame it opens; keep it focused thereafter so
                    // typing never escapes to the global single-key handlers.
                    if pal.just_opened {
                        field.response.request_focus();
                        pal.just_opened = false;
                    } else if !field.response.has_focus() {
                        field.response.request_focus();
                    }
                });

                ui.add_space(8.0);
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, theme.glass_border),
                );
                ui.add_space(6.0);

                // ── scored result list ─────────────────────────────────────────────
                if n == 0 {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new("No matches")
                                .size(13.0)
                                .color(theme.muted),
                        );
                    });
                    ui.add_space(10.0);
                } else {
                    // The GLIDING gold edge: animate the selected index, then paint the
                    // 2px edge at the interpolated row position. One signal, learned once.
                    let row_h = 38.0;
                    let glide = ctx.animate_value_with_time(
                        egui::Id::new("palette-glide"),
                        pal.selected as f32,
                        0.10,
                    );
                    let list_top = ui.cursor().top();

                    egui::ScrollArea::vertical()
                        .max_height(360.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            for (rank, &(_, idx)) in scored.iter().enumerate() {
                                let c = &all[idx];
                                let sel = rank == pal.selected;
                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), row_h),
                                    Sense::click(),
                                );
                                let p = ui.painter();
                                // 0.10 selected wash (the edge carries the signal).
                                if sel {
                                    p.rect_filled(
                                        rect.shrink2(egui::vec2(2.0, 2.0)),
                                        6.0,
                                        GOLD.gamma_multiply(0.10),
                                    );
                                } else if resp.hovered() {
                                    p.rect_filled(rect.shrink2(egui::vec2(2.0, 2.0)), 6.0, theme.hover);
                                }
                                // Leading glyph.
                                p.text(
                                    egui::pos2(rect.left() + 16.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    c.glyph,
                                    FontId::proportional(17.0),
                                    if sel { theme.gold_ink } else { theme.muted },
                                );
                                // Primary label.
                                p.text(
                                    egui::pos2(rect.left() + 42.0, rect.center().y),
                                    Align2::LEFT_CENTER,
                                    &c.label,
                                    FontId::new(13.5, icons::semibold()),
                                    if sel { theme.ink } else { theme.ink },
                                );
                                // Right cluster: shortcut hint then the category chip.
                                let mut rx = rect.right() - 14.0;
                                if !c.shortcut.is_empty() {
                                    let galley = p.layout_no_wrap(
                                        c.shortcut.clone(),
                                        FontId::monospace(11.5),
                                        theme.faint,
                                    );
                                    rx -= galley.size().x;
                                    p.galley(
                                        egui::pos2(rx, rect.center().y - galley.size().y / 2.0),
                                        galley,
                                        theme.faint,
                                    );
                                    rx -= 12.0;
                                }
                                let chip = p.layout_no_wrap(
                                    c.category.to_string(),
                                    FontId::proportional(11.0),
                                    theme.faint,
                                );
                                rx -= chip.size().x;
                                p.galley(
                                    egui::pos2(rx, rect.center().y - chip.size().y / 2.0),
                                    chip,
                                    theme.faint,
                                );
                                if resp.clicked() {
                                    click_idx = Some(rank);
                                }
                            }
                        });

                    // Paint the gliding 2px gold LEFT edge over the (un-scrolled) list.
                    // Rows fit within max_height for the common (short) command set, so
                    // the simple interpolated y is correct without scroll math here.
                    let edge_y = list_top + glide * row_h;
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            egui::pos2(ui.max_rect().left() + 2.0, edge_y + 3.0),
                            egui::pos2(ui.max_rect().left() + 4.0, edge_y + row_h - 3.0),
                        ),
                        1.0,
                        theme.gold_tick,
                    );
                }

                // Footer hint.
                ui.add_space(6.0);
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, theme.glass_border),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("↑↓ navigate    ⏎ run    esc close")
                            .size(11.0)
                            .color(theme.faint),
                    );
                });
            });
        });

    // Resolve a chosen command (Enter on the selection, or a clicked row).
    let pick = if run {
        Some(pal.selected)
    } else {
        click_idx
    };
    if let Some(rank) = pick {
        if let Some(&(_, idx)) = scored.get(rank) {
            chosen = Some(all[idx].action.clone());
        }
    }

    // Keep the live UI state (query + selection) unless we ran something.
    if chosen.is_some() {
        state.palette = None;
    } else {
        state.palette = Some(pal);
    }
    chosen
}

/// The "?" keyboard cheat-sheet — a palette-styled help card listing the FULL key
/// map (the complete version of the P2 stub), grouped into Navigate / Panels /
/// Lists & selection / Per-section / Compose. Dismisses on Esc (the ladder) or a
/// click on the close affordance. Returns true if the user asked to close it.
pub fn cheat_sheet(ctx: &egui::Context, theme: &Theme) -> bool {
    let m = cmd_mod();
    // (section header, [(label, keys)]). Owned Vec so the format!-built key strings
    // outlive the closures below.
    let groups: Vec<(&str, Vec<(&str, String)>)> = vec![
        (
            "Navigate",
            vec![
                ("Command palette — run anything", format!("{m}K")),
                ("Chat · Feed · Wallet", format!("{m}1 · {m}2 · {m}3")),
                ("Verse · Calls · You", format!("{m}4 · {m}5 · {m}6")),
                ("Cycle pane focus", format!("{m}[ · {m}] · Tab")),
                ("Search the active pane", format!("{m}F · /")),
            ],
        ),
        (
            "Panels & view",
            vec![
                ("Toggle info panel", format!("{m}\\")),
                ("Toggle list column", format!("{m}B")),
                ("Settings", format!("{m},")),
                ("Toggle theme", format!("{m}D")),
            ],
        ),
        (
            "Lists & selection",
            vec![
                ("Move selection", "J · K · ↑ · ↓".into()),
                ("Open / activate selected", "⏎".into()),
                ("Right-click for context menu", "—".into()),
            ],
        ),
        (
            "Per-section",
            vec![
                ("Chat: mute · pin · delete", "M · P · Del".into()),
                ("Message: reply · edit · delete", "R · E · Del".into()),
                ("Feed: like · comment · new post", format!("L · C · {m}N")),
                ("Wallet: send · receive", "S · R".into()),
                ("Calls: mute · camera · hang up", "M · V · Esc".into()),
            ],
        ),
        (
            "Compose & overlays",
            vec![
                ("Send · newline", "⏎ · ⇧⏎".into()),
                ("Edit last own message", "↑ (empty field)".into()),
                ("This help", "?".into()),
                ("Close / peel overlay", "Esc".into()),
            ],
        ),
    ];
    let mut close = false;
    egui::Area::new(egui::Id::new("cheat-sheet"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            theme.floating(10.0).show(ui, |ui| {
                ui.set_width(480.0);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Keyboard shortcuts")
                            .size(15.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::views::icon_button(ui, theme, icons::CLOSE, 16.0, theme.muted).clicked() {
                            close = true;
                        }
                    });
                });
                ui.add_space(8.0);
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    ui.cursor().top(),
                    Stroke::new(1.0, theme.glass_border),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .max_height(440.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for (gi, (header, rows)) in groups.iter().enumerate() {
                            if gi > 0 {
                                ui.add_space(8.0);
                            }
                            // Caps eyebrow (uppercase + smaller, no true tracking in egui).
                            ui.label(
                                RichText::new(header.to_uppercase())
                                    .size(10.5)
                                    .family(icons::medium())
                                    .color(theme.faint),
                            );
                            ui.add_space(4.0);
                            for (label, keys) in rows.iter() {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(*label).size(13.0).color(theme.muted));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            RichText::new(keys)
                                                .text_style(egui::TextStyle::Monospace)
                                                .color(theme.ink),
                                        );
                                    });
                                });
                                ui.add_space(3.0);
                            }
                        }
                    });
            });
        });
    close
}
