//! Wallet tab — a desktop port of the Android WalletScreen. A sovereign,
//! multi-chain, self-custody wallet: the keys are derived in-process by the
//! embedded runtime (BIP39 → secp256k1 for EVM, P-256 for Elastos), so this view
//! only ever holds public addresses + fetched balances and dispatches signed
//! sends through `walletops` on an engine worker.
//!
//! Chains: ESC (Elastos Smart Chain) + Ethereum (EVM, shared 0x… address, native
//! coin + curated ERC-20s) and the Elastos ELA mainchain (UTXO, E… address). BEAM
//! is deferred (it needs the C++ wallet-core). Every send goes through a confirm
//! screen that mints + redeems a one-shot spend grant (guard.rs audit trail).

use egui::{Color32, RichText, Sense, Stroke, TextureHandle};
use serde_json::Value;

use crate::app::App;
use crate::icons;
use crate::state::{SendForm, SendStage, TipForm, TipStage};
use crate::theme::{Theme, GOLD, LIKE, NAVY};

pub fn ui(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    // Lazy unlock on first open (guarded by a one-shot memory flag).
    let load_id = egui::Id::new("wallet-load-started");
    let started = ui.ctx().memory(|m| m.data.get_temp::<bool>(load_id).unwrap_or(false));
    if !app.state.wallet.loaded && !started {
        ui.ctx().memory_mut(|m| m.data.insert_temp(load_id, true));
        app.load_wallet();
    }

    // Identity predates the wallet (no BIP39 seed) → offer to create one.
    if app.state.wallet.locked {
        locked_panel(app, ui, theme);
        return;
    }

    if !app.state.wallet.loaded {
        ui.add_space(90.0);
        ui.vertical_centered(|ui| {
            ui.spinner();
            ui.add_space(10.0);
            ui.label(RichText::new("Unlocking your wallet…").size(14.0).color(theme.muted));
        });
        return;
    }

    // Snapshot the bits we read so app.state is free for dispatch below.
    let chain = app.state.wallet.chain.clone();
    let chains = app.state.wallet.chains.clone();
    let evm_addr = app.state.wallet.evm_addr.clone();
    let ela_addr = app.state.wallet.ela_addr.clone();
    let did = app.state.wallet.did.clone();
    let refreshing = app.state.wallet.refreshing.contains(&chain);
    let bal = app.state.wallet.balances.get(&chain).cloned();
    let show_history = app.state.wallet.show_history;
    let history = app.state.wallet.history.clone();
    let hidden = app.state.wallet.hidden_tokens.clone();
    let show_hidden = app.state.wallet.show_hidden;

    // Intents collected during the immediate-mode body, applied after.
    let mut select_chain: Option<String> = None;
    let mut open_receive: Option<String> = None;
    let mut open_send: Option<(String, Option<Value>)> = None;
    let mut refresh_chain: Option<String> = None;
    let mut copy: Option<String> = None;
    let mut toggle_history = false;
    let mut open_backup = false;
    let mut open_settings = false;
    let mut toggle_hidden = false;
    let mut hide_token: Option<(String, bool)> = None; // (contract, hidden)

    let avail = ui.available_width();
    let col_w = avail.min(720.0);
    let pad = ((avail - col_w) * 0.5).max(0.0);

    let out = egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
                ui.set_width(col_w);

                // Wallet header: title + a gear → settings sheet (parity with Android).
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Wallet").size(22.0).family(icons::display()).color(theme.ink));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::icon_button(ui, theme, icons::SETTINGS, 20.0, theme.muted)
                            .on_hover_text("Wallet settings")
                            .clicked()
                        {
                            open_settings = true;
                        }
                    });
                });
                ui.add_space(10.0);

                identity_card(ui, theme, &did, &evm_addr, &ela_addr, &mut open_backup, &mut copy);
                ui.add_space(14.0);

                chain_selector(ui, theme, &chains, &chain, &mut select_chain);
                ui.add_space(14.0);

                balance_card(
                    ui, theme, &chain, &evm_addr, &ela_addr, bal.as_ref(), refreshing,
                    &mut open_receive, &mut open_send, &mut refresh_chain, &mut copy,
                );

                // Curated ERC-20 tokens (EVM chains only).
                if chain != "ela" {
                    let tokens = tokens_of(bal.as_ref());
                    if !tokens.is_empty() {
                        ui.add_space(14.0);
                        token_list(
                            ui, theme, &tokens, &chain, &hidden, show_hidden,
                            &mut open_send, &mut hide_token, &mut toggle_hidden,
                            &mut open_receive, &mut copy,
                        );
                    }
                }

                ui.add_space(14.0);
                history_section(ui, theme, &history, show_history, &mut toggle_history, &chain, &mut copy);

                ui.add_space(18.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Self-custody · all keys derived on this device from one BIP39 seed")
                            .size(11.0)
                            .color(theme.muted),
                    );
                });
                ui.add_space(40.0);
            });
        });
    });
    // Feed the collapsing large-title header (§4d).
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("view-scroll-y"), out.state.offset.y));

    // ── apply intents ──────────────────────────────────────────────────────────
    if let Some(c) = select_chain {
        if c != app.state.wallet.chain {
            app.state.wallet.chain = c.clone();
            if !app.state.wallet.balances.contains_key(&c) {
                app.state.wallet.refreshing.insert(c.clone());
                app.load_wallet_balance(&c);
            }
        }
    }
    if let Some(c) = refresh_chain {
        app.state.wallet.refreshing.insert(c.clone());
        app.load_wallet_balance(&c);
    }
    if let Some(c) = open_receive {
        app.state.wallet.receive = Some(c);
    }
    if let Some((c, tok)) = open_send {
        app.state.wallet.send = SendForm { open: true, chain: c, token: tok, ..Default::default() };
    }
    if toggle_history {
        app.state.wallet.show_history = !app.state.wallet.show_history;
    }
    if open_backup {
        app.state.wallet.show_backup = true;
    }
    if open_settings {
        app.state.wallet.show_settings = true;
    }
    if toggle_hidden {
        app.state.wallet.show_hidden = !app.state.wallet.show_hidden;
    }
    if let Some((contract, hide)) = hide_token {
        app.set_token_hidden(&chain, &contract, hide);
    }
    if let Some(t) = copy {
        ui.output_mut(|o| o.copied_text = t);
        let now = ui.ctx().input(|i| i.time);
        app.state.toast = Some(("Copied".into(), now + 1.5));
    }

    // Overlays (rendered on the context so they float above the panel).
    receive_sheet(app, ui.ctx(), theme);
    send_sheet(app, ui.ctx(), theme);
    backup_sheet(app, ui.ctx(), theme);
    settings_sheet(app, ui.ctx(), theme);
}

// ── identity card (DID + Elastos accounts, one seed) ──────────────────────────
fn identity_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    did: &str,
    evm_addr: &str,
    ela_addr: &str,
    open_backup: &mut bool,
    copy: &mut Option<String>,
) {
    theme.material_raised(18.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(icons::VERIFIED_USER).size(18.0).color(theme.gold_ink));
            ui.add_space(6.0);
            ui.label(
                RichText::new("Elastos Identity")
                    .size(13.0)
                    .family(icons::semibold())
                    .color(theme.ink),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::secondary_button(ui, theme, false, &format!("{}  Back up", icons::KEY)).clicked() {
                    *open_backup = true;
                }
            });
        });
        ui.add_space(10.0);
        if did.is_empty() {
            ui.label(RichText::new("deriving identity…").size(13.0).color(theme.muted));
        } else {
            addr_row(ui, theme, "DID", did, copy);
        }
        // Mainchain derivation can independently fail (unwrap_or_default → ""); only
        // show a row when the address actually resolved.
        if !ela_addr.is_empty() {
            addr_row(ui, theme, "Mainchain", ela_addr, copy);
        }
        if !evm_addr.is_empty() {
            addr_row(ui, theme, "Smart Chain", evm_addr, copy);
        }
        ui.add_space(6.0);
        ui.label(
            RichText::new("One Hey seed · the same DID, ELA mainchain & ESC/EID accounts as Elastos Essentials")
                .size(11.0)
                .color(theme.muted),
        );
    });
}

/// A label + shortened value + copy button row (used in the identity card).
fn addr_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str, copy: &mut Option<String>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(12.0).color(theme.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if super::icon_button(ui, theme, icons::CONTENT_COPY, 13.0, theme.muted)
                .on_hover_text("Copy")
                .clicked()
            {
                *copy = Some(value.to_string());
            }
            ui.label(RichText::new(short_addr(value)).size(12.0).color(theme.ink).monospace());
        });
    });
    ui.add_space(4.0);
}

// ── recovery-phrase backup sheet ──────────────────────────────────────────────
fn backup_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if !app.state.wallet.show_backup {
        return;
    }
    let phrase = app.state.wallet.phrase.clone();
    let screen = ctx.screen_rect();
    let mut close = false;
    let mut reveal = false;
    let mut copy: Option<String> = None;

    egui::Window::new("wallet-backup")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).min(460.0));
            theme.sheet_handle(ui);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  Recovery phrase", icons::KEY))
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(10.0);

            // Warning banner.
            egui::Frame::none()
                .fill(LIKE.gamma_multiply(0.12))
                .rounding(10.0)
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "{}  Anyone with these words controls your funds. Never share them or type them into a website.",
                            icons::SHIELD
                        ))
                        .size(12.0)
                        .color(theme.ink),
                    );
                });
            ui.add_space(12.0);

            match &phrase {
                None => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(14.0);
                        ui.label(RichText::new("Your recovery phrase is hidden.").size(13.0).color(theme.muted));
                        ui.add_space(12.0);
                        if super::primary_button(ui, false, "Reveal recovery phrase").clicked() {
                            reveal = true;
                        }
                        ui.add_space(14.0);
                    });
                }
                Some(p) => {
                    word_grid(ui, theme, p);
                    ui.add_space(10.0);
                    if super::secondary_button(ui, theme, false, &format!("{}  Copy phrase", icons::CONTENT_COPY)).clicked() {
                        copy = Some(p.clone());
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("These words restore your DID, ELA mainchain, ESC & EID in Elastos Essentials (BIP39, m/44').")
                            .size(11.0)
                            .color(theme.muted),
                    );
                }
            }

            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::primary_button(ui, false, "Done").clicked() {
                    close = true;
                }
            });
        });

    if reveal && app.state.wallet.phrase.is_none() {
        app.load_wallet_phrase();
    }
    if let Some(c) = copy {
        ctx.output_mut(|o| o.copied_text = c);
        let now = ctx.input(|i| i.time);
        app.state.toast = Some(("Recovery phrase copied".into(), now + 2.0));
    }
    if close {
        // Drop the phrase from memory when the sheet closes.
        app.state.wallet.show_backup = false;
        app.state.wallet.phrase = None;
    }
}

// ── wallet settings sheet (the gear) ──────────────────────────────────────────
//
// A desktop port of the Android `WalletSettingsSheet`: transaction-history
// show/hide + show-hidden-tokens. BEAM (private wallet) is N/A on desktop — there's
// no C++ wallet-core here — so it's omitted. Relay selection lives in the
// Connection sheet (Profile tab), so it's not duplicated here, just pointed to.
fn settings_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if !app.state.wallet.show_settings {
        return;
    }
    let show_history = app.state.wallet.show_history;
    let show_hidden = app.state.wallet.show_hidden;
    let hidden_n = app.state.wallet.hidden_tokens.len();
    let screen = ctx.screen_rect();
    let mut close = false;
    let mut toggle_history = false;
    let mut toggle_hidden = false;

    egui::Window::new("wallet-settings")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).min(440.0));
            theme.sheet_handle(ui);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{}  Wallet settings", icons::SETTINGS))
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(12.0);

            // Transaction history show/hide.
            setting_row(
                ui,
                theme,
                icons::RECEIPT_LONG,
                "Show transaction history",
                "Your sends + tips (received payments coming soon).",
                show_history,
                "wallet-set-hist",
                &mut toggle_history,
            );
            ui.add_space(10.0);

            // Show hidden tokens.
            setting_row(
                ui,
                theme,
                icons::VISIBILITY,
                "Show hidden tokens",
                &if hidden_n == 0 {
                    "Hidden scam/dust tokens reappear in the token list.".to_string()
                } else {
                    format!("{hidden_n} hidden — reveal them to unhide in the token list.")
                },
                show_hidden,
                "wallet-set-hidden",
                &mut toggle_hidden,
            );
            ui.add_space(12.0);

            // BEAM N/A + relay pointer (parity notes).
            ui.label(
                RichText::new("BEAM private wallet isn't available on desktop. Network & relay settings live in Connection (Profile tab).")
                    .size(11.0)
                    .color(theme.muted),
            );

            ui.add_space(16.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::primary_button(ui, false, "Done").clicked() {
                    close = true;
                }
            });
        });

    if toggle_history {
        app.state.wallet.show_history = !app.state.wallet.show_history;
    }
    if toggle_hidden {
        app.state.wallet.show_hidden = !app.state.wallet.show_hidden;
    }
    if close {
        app.state.wallet.show_settings = false;
    }
}

/// A glass settings row: leading icon, title + subtitle, trailing iOS switch.
#[allow(clippy::too_many_arguments)]
fn setting_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    glyph: &str,
    title: &str,
    body: &str,
    on: bool,
    id: &str,
    toggle: &mut bool,
) {
    theme.glass(14.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(glyph).size(20.0).color(theme.gold_ink));
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width() - 60.0);
                ui.label(RichText::new(title).size(14.0).family(icons::semibold()).color(theme.ink));
                ui.label(RichText::new(body).size(12.0).color(theme.muted));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if super::switch(ui, theme, id, on).clicked() {
                    *toggle = true;
                }
            });
        });
    });
}

/// The phrase as numbered word cells, 3 per row.
fn word_grid(ui: &mut egui::Ui, theme: &Theme, phrase: &str) {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    for (row, chunk) in words.chunks(3).enumerate() {
        ui.horizontal(|ui| {
            for (col, w) in chunk.iter().enumerate() {
                let n = row * 3 + col + 1;
                // Recessed tonal tile (radius 8) — calm, no hard hairline.
                egui::Frame::none()
                    .fill(theme.hover)
                    .rounding(8.0)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_width(116.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{n}")).size(11.0).color(theme.faint));
                            ui.add_space(4.0);
                            ui.label(RichText::new(*w).size(13.0).family(icons::semibold()).color(theme.ink).monospace());
                        });
                    });
            }
        });
        ui.add_space(6.0);
    }
}

// ── create-seed (locked identity) panel ───────────────────────────────────────
fn locked_panel(app: &mut App, ui: &mut egui::Ui, theme: &Theme) {
    let avail = ui.available_width();
    let col_w = avail.min(560.0);
    let pad = ((avail - col_w) * 0.5).max(0.0);
    let creating = app.state.wallet.creating_seed;

    ui.add_space(48.0);
    ui.horizontal(|ui| {
        ui.add_space(pad);
        ui.vertical(|ui| {
            ui.set_width(col_w);
            theme.material_raised(18.0).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(icons::KEY).size(38.0).color(theme.gold_ink));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Set up your wallet")
                            .size(22.0)
                            .family(icons::semibold())
                            .color(theme.ink),
                    );
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(
                            "This device's identity was created before the wallet and has no \
                             recovery phrase. Generate a new BIP39 seed to use the Elastos \
                             mainchain (ELA) and ESC — the same seed restores in Elastos Essentials.",
                        )
                        .size(13.0)
                        .color(theme.muted),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("This creates a new Hey identity on this device.")
                            .size(11.0)
                            .color(theme.muted),
                    );
                    ui.add_space(18.0);
                    if creating {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.add_space(8.0);
                            ui.label(RichText::new("Creating your seed… the app will restart.").size(13.0).color(theme.gold_ink));
                        });
                    } else if super::primary_button(ui, false, "Create wallet seed").clicked() {
                        app.state.wallet.creating_seed = true;
                        app.create_wallet_seed();
                    }
                    ui.add_space(10.0);
                });
            });
        });
    });
}

// ── chain selector ────────────────────────────────────────────────────────────
/// The chain registry as a sliding segmented control (replaces the pills). The EVM
/// registry chains come first; the ELA mainchain is appended (it is not in the
/// registry). The selected index maps back to its chain key.
fn chain_selector(ui: &mut egui::Ui, theme: &Theme, chains: &[Value], sel: &str, out: &mut Option<String>) {
    // Build the (key, short-label) list in display order.
    let mut keys: Vec<&str> = chains
        .iter()
        .filter_map(|c| c.get("key").and_then(Value::as_str))
        .filter(|k| !k.is_empty())
        .collect();
    keys.push("ela"); // mainchain — not in the EVM registry
    if keys.is_empty() {
        return;
    }

    let labels: Vec<&str> = keys.iter().map(|k| chain_short(k)).collect();
    let selected = keys.iter().position(|k| *k == sel).unwrap_or(0);

    if let Some(i) = super::segmented(ui, theme, "wallet-chain", &labels, selected) {
        if let Some(k) = keys.get(i) {
            *out = Some(k.to_string());
        }
    }
}

// ── balance card ──────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn balance_card(
    ui: &mut egui::Ui,
    theme: &Theme,
    chain: &str,
    evm_addr: &str,
    ela_addr: &str,
    bal: Option<&Value>,
    refreshing: bool,
    open_receive: &mut Option<String>,
    open_send: &mut Option<(String, Option<Value>)>,
    refresh_chain: &mut Option<String>,
    copy: &mut Option<String>,
) {
    let (symbol, amount) = native_of(chain, bal);
    let addr = if chain == "ela" { ela_addr } else { evm_addr };

    theme.material_raised(18.0).show(ui, |ui| {
        ui.set_width(ui.available_width());

        ui.horizontal(|ui| {
            ui.label(RichText::new(chain_full(chain)).size(13.0).color(theme.muted));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if refreshing {
                    ui.add(egui::Spinner::new().size(14.0));
                } else if super::icon_button(ui, theme, icons::REFRESH, 16.0, theme.muted)
                    .on_hover_text("Refresh balance")
                    .clicked()
                {
                    *refresh_chain = Some(chain.to_string());
                }
            });
        });

        ui.add_space(6.0);
        // Big balance — display weight, gold symbol (the hero number of the view).
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&amount)
                    .size(30.0)
                    .family(icons::display())
                    .color(theme.ink),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(&symbol)
                    .size(16.0)
                    .family(icons::semibold())
                    .color(theme.gold_ink),
            );
        });

        ui.add_space(8.0);
        // Address row (short, click the chip to copy).
        ui.horizontal(|ui| {
            ui.label(RichText::new(short_addr(addr)).size(12.0).color(theme.muted).monospace());
            ui.add_space(2.0);
            if super::icon_button(ui, theme, icons::CONTENT_COPY, 14.0, theme.muted)
                .on_hover_text("Copy address")
                .clicked()
            {
                *copy = Some(addr.to_string());
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            // Send (primary, flat gold).
            if super::primary_button(ui, false, &format!("{}  Send", icons::ARROW_UPWARD))
                .on_hover_text("Send  S")
                .clicked()
            {
                *open_send = Some((chain.to_string(), None));
            }
            ui.add_space(10.0);
            // Receive (calmest outline style).
            if super::outline_button(ui, theme, false, &format!("{}  Receive", icons::ARROW_DOWNWARD))
                .on_hover_text("Receive  R")
                .clicked()
            {
                *open_receive = Some(chain.to_string());
            }
        });
    });
}

// ── token list (EVM) ──────────────────────────────────────────────────────────
//
// Curated ERC-20s, filtered through the per-user HIDDEN set (== Android scam/dust
// protection): hidden tokens drop out of the default list, with a "Show hidden"
// toggle to reveal them and a per-row hide/unhide affordance (the eye button). The
// native coin is never hideable (no contract). `hidden` keys are "chain:contract".
#[allow(clippy::too_many_arguments)]
fn token_list(
    ui: &mut egui::Ui,
    theme: &Theme,
    tokens: &[Value],
    chain: &str,
    hidden: &std::collections::HashSet<String>,
    show_hidden: bool,
    open_send: &mut Option<(String, Option<Value>)>,
    hide_token: &mut Option<(String, bool)>,
    toggle_hidden: &mut bool,
    open_receive: &mut Option<String>,
    copy: &mut Option<String>,
) {
    let is_hidden = |contract: &str| !contract.is_empty() && hidden.contains(&format!("{chain}:{contract}"));
    let hidden_n = tokens
        .iter()
        .filter(|t| is_hidden(t.get("contract").and_then(Value::as_str).unwrap_or("")))
        .count();
    // The visible rows: all tokens unless one is hidden and we're not revealing.
    let visible: Vec<&Value> = tokens
        .iter()
        .filter(|t| {
            let c = t.get("contract").and_then(Value::as_str).unwrap_or("");
            show_hidden || !is_hidden(c)
        })
        .collect();

    section_label(ui, theme, "Tokens");
    ui.add_space(6.0);
    if visible.is_empty() {
        // Every token is hidden and we're not revealing — show only the toggle.
        if hidden_n > 0 {
            hidden_toggle(ui, theme, show_hidden, hidden_n, toggle_hidden);
        }
        return;
    }
    theme.glass(16.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (i, t) in visible.iter().enumerate() {
            if i > 0 {
                row_divider(ui, theme);
            }
            let symbol = t.get("symbol").and_then(Value::as_str).unwrap_or("");
            let name = t.get("name").and_then(Value::as_str).unwrap_or("");
            let balance = t.get("balance").and_then(Value::as_str).unwrap_or("0");
            let contract = t.get("contract").and_then(Value::as_str).unwrap_or("").to_string();
            let row_hidden = is_hidden(&contract);
            let mut toggled: Option<bool> = None;
            let resp = super::list_row(ui, theme, false, |ui| {
                ui.horizontal(|ui| {
                    // token glyph chip
                    let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), Sense::hover());
                    let chip = if row_hidden { GOLD.gamma_multiply(0.08) } else { GOLD.gamma_multiply(0.18) };
                    ui.painter().circle_filled(r.center(), 17.0, chip);
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        symbol.chars().next().unwrap_or('?').to_string(),
                        egui::FontId::proportional(15.0),
                        theme.gold_ink,
                    );
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new(symbol).size(14.0).family(icons::semibold()).color(theme.ink));
                        ui.label(RichText::new(name).size(11.0).color(theme.muted));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Hide / unhide affordance (the eye) — only for ERC-20s, never
                        // the native coin (it has no contract). Clicking it must NOT
                        // also open Send, so it's a nested icon button that swallows.
                        if !contract.is_empty() {
                            let (glyph, tip) = if row_hidden {
                                (icons::VISIBILITY, "Unhide")
                            } else {
                                (icons::VISIBILITY_OFF, "Hide (scam protection)")
                            };
                            if super::icon_button(ui, theme, glyph, 16.0, theme.muted)
                                .on_hover_text(tip)
                                .clicked()
                            {
                                toggled = Some(!row_hidden);
                            }
                            ui.add_space(4.0);
                        }
                        ui.label(RichText::new(balance).size(14.0).color(theme.ink));
                    });
                });
            });
            // Right-click → Send / Receive / Hide / Copy address.
            let tok = (*t).clone();
            resp.context_menu(|ui| {
                ui.set_min_width(180.0);
                if super::menu_item(ui, theme, icons::ARROW_UPWARD, "Send", "S", false).clicked() {
                    *open_send = Some((chain.to_string(), Some(tok.clone())));
                    ui.close_menu();
                }
                if super::menu_item(ui, theme, icons::ARROW_DOWNWARD, "Receive", "R", false).clicked() {
                    *open_receive = Some(chain.to_string());
                    ui.close_menu();
                }
                if !contract.is_empty() {
                    let (hglyph, hlabel) = if row_hidden {
                        (icons::VISIBILITY, "Unhide")
                    } else {
                        (icons::VISIBILITY_OFF, "Hide")
                    };
                    if super::menu_item(ui, theme, hglyph, hlabel, "", false).clicked() {
                        toggled = Some(!row_hidden);
                        ui.close_menu();
                    }
                    ui.add_space(2.0);
                    if super::menu_item(ui, theme, icons::CONTENT_COPY, "Copy address", "", false).clicked() {
                        *copy = Some(contract.clone());
                        ui.close_menu();
                    }
                }
            });
            if let Some(h) = toggled {
                *hide_token = Some((contract.clone(), h));
            } else if resp.clicked() {
                *open_send = Some((chain.to_string(), Some((*t).clone())));
            }
        }
    });
    if hidden_n > 0 {
        ui.add_space(6.0);
        hidden_toggle(ui, theme, show_hidden, hidden_n, toggle_hidden);
    }
    ui.add_space(6.0);
    ui.label(
        RichText::new("Only curated tokens show here. Hide any you didn't ask for — a scammer can airdrop a fake token, but it can't move your funds.")
            .size(11.0)
            .color(theme.muted),
    );
}

/// The "Show N hidden" / "Hide hidden tokens" reveal toggle row under the token list.
fn hidden_toggle(ui: &mut egui::Ui, theme: &Theme, show_hidden: bool, hidden_n: usize, toggle: &mut bool) {
    let (glyph, label) = if show_hidden {
        (icons::VISIBILITY_OFF, "Hide hidden tokens".to_string())
    } else {
        (icons::VISIBILITY, format!("Show {hidden_n} hidden"))
    };
    if ui
        .add(
            egui::Button::new(
                RichText::new(format!("{glyph}  {label}")).size(12.0).family(icons::semibold()).color(theme.muted),
            )
            .fill(Color32::TRANSPARENT),
        )
        .clicked()
    {
        *toggle = true;
    }
}

// ── history ───────────────────────────────────────────────────────────────────
#[allow(clippy::too_many_arguments)]
fn history_section(
    ui: &mut egui::Ui,
    theme: &Theme,
    history: &[Value],
    show: bool,
    toggle: &mut bool,
    chain: &str,
    copy: &mut Option<String>,
) {
    ui.horizontal(|ui| {
        section_label(ui, theme, &format!("{}  Activity", icons::RECEIPT_LONG));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let label = if show { "Hide" } else { "Show" };
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(label).size(12.0).family(icons::semibold()).color(theme.gold_ink),
                    )
                    .fill(Color32::TRANSPARENT),
                )
                .clicked()
            {
                *toggle = true;
            }
        });
    });
    if !show {
        return;
    }
    ui.add_space(6.0);
    if history.is_empty() {
        theme.glass(16.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("No transactions yet").size(13.0).color(theme.muted));
                ui.label(RichText::new("Sent transfers + tips will appear here.").size(11.0).color(theme.muted));
                ui.add_space(8.0);
            });
        });
        return;
    }
    let now = super::now_ms();
    theme.glass(16.0).show(ui, |ui| {
        ui.set_width(ui.available_width());
        for (i, rec) in history.iter().enumerate() {
            if i > 0 {
                row_divider(ui, theme);
            }
            let symbol = rec.get("symbol").and_then(Value::as_str).unwrap_or("");
            let to = rec.get("to").and_then(Value::as_str).unwrap_or("");
            let amount = rec.get("amount").and_then(Value::as_str).unwrap_or("");
            let kind = rec.get("kind").and_then(Value::as_str).unwrap_or("sent");
            let ts = rec.get("ts").and_then(Value::as_i64).unwrap_or(0);
            let hash = rec.get("hash").and_then(Value::as_str).unwrap_or("").to_string();
            let resp = super::list_row(ui, theme, false, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 15.0, theme.hover);
                    ui.painter().text(
                        r.center(),
                        egui::Align2::CENTER_CENTER,
                        icons::ARROW_UPWARD,
                        egui::FontId::proportional(15.0),
                        theme.muted,
                    );
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        let verb = if kind == "tip" { "Tip" } else { "Sent" };
                        ui.label(RichText::new(format!("{verb} to {}", short_addr(to))).size(13.0).color(theme.ink));
                        if ts > 0 {
                            ui.label(RichText::new(super::rel_time(ts, now)).size(11.0).color(theme.muted));
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("-{amount} {symbol}")).size(13.0).family(icons::semibold()).color(theme.ink));
                    });
                });
            });
            // Right-click a tx row → Copy hash / View on explorer / Copy recipient.
            let to_owned = to.to_string();
            resp.context_menu(|ui| {
                ui.set_min_width(180.0);
                if !hash.is_empty() {
                    if super::menu_item(ui, theme, icons::CONTENT_COPY, "Copy hash", "", false).clicked() {
                        *copy = Some(hash.clone());
                        ui.close_menu();
                    }
                    if let Some(url) = explorer_tx_url(chain, &hash) {
                        if super::menu_item(ui, theme, icons::LINK, "View on explorer", "", false).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            ui.close_menu();
                        }
                    }
                    ui.add_space(2.0);
                }
                if !to_owned.is_empty()
                    && super::menu_item(ui, theme, icons::PERSON, "Copy recipient", "", false).clicked()
                {
                    *copy = Some(to_owned.clone());
                    ui.close_menu();
                }
            });
        }
    });
}

/// A block-explorer transaction URL for a chain's tx hash, when one is known.
/// Used by the tx-row "View on explorer" context action.
fn explorer_tx_url(chain: &str, hash: &str) -> Option<String> {
    let base = match chain {
        "esc" => "https://esc.elastos.io/tx/",
        "eid" => "https://eid.elastos.io/tx/",
        "ethereum" => "https://etherscan.io/tx/",
        "ela" => "https://ela.elastos.io/tx/",
        _ => return None,
    };
    Some(format!("{base}{hash}"))
}

// ── receive sheet ─────────────────────────────────────────────────────────────
fn receive_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    let Some(chain) = app.state.wallet.receive.clone() else {
        return;
    };
    let addr = if chain == "ela" {
        app.state.wallet.ela_addr.clone()
    } else {
        app.state.wallet.evm_addr.clone()
    };
    let mut close = false;
    let mut copy: Option<String> = None;
    let screen = ctx.screen_rect();

    egui::Window::new("wallet-receive")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).min(380.0));
            theme.sheet_handle(ui);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Receive {}", native_symbol(&chain)))
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(12.0);
            ui.vertical_centered(|ui| {
                if let Some(tex) = qr_texture(ctx, &addr) {
                    let side = 200.0;
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .rounding(12.0)
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Image::new(egui::load::SizedTexture::from_handle(&tex))
                                    .fit_to_exact_size(egui::vec2(side, side)),
                            );
                        });
                }
                ui.add_space(12.0);
                ui.label(RichText::new(&addr).size(12.0).color(theme.ink).monospace());
                ui.add_space(10.0);
                if super::primary_button(ui, false, &format!("{}  Copy address", icons::CONTENT_COPY)).clicked() {
                    copy = Some(addr.clone());
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Only send {} on {} to this address.", native_symbol(&chain), chain_full(&chain)))
                        .size(11.0)
                        .color(theme.muted),
                );
            });
        });

    if let Some(a) = copy {
        ctx.output_mut(|o| o.copied_text = a);
        let now = ctx.input(|i| i.time);
        app.state.toast = Some(("Address copied".into(), now + 1.5));
    }
    if close {
        app.state.wallet.receive = None;
    }
}

// ── send sheet ────────────────────────────────────────────────────────────────
fn send_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if !app.state.wallet.send.open {
        return;
    }
    let chain = app.state.wallet.send.chain.clone();
    let token = app.state.wallet.send.token.clone();
    let stage = app.state.wallet.send.stage.clone();
    let symbol = token
        .as_ref()
        .and_then(|t| t.get("symbol").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| native_symbol(&chain).to_string());
    let screen = ctx.screen_rect();

    let mut close = false;
    let mut do_send: Option<(String, Option<Value>, String, String)> = None;

    egui::Window::new("wallet-send")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).min(440.0));

            // header
            theme.sheet_handle(ui);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("Send {symbol}"))
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked() {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, theme.glass_border));
            ui.add_space(12.0);

            match stage {
                SendStage::Edit => {
                    ui.label(RichText::new(format!("Recipient ({} address)", chain_full(&chain))).size(12.0).color(theme.muted));
                    ui.add_space(4.0);
                    let to_resp = ui.add(
                        egui::TextEdit::singleline(&mut app.state.wallet.send.to)
                            .desired_width(ui.available_width())
                            .margin(egui::Margin::symmetric(14.0, 12.0))
                            .font(egui::FontId::proportional(17.0))
                            .hint_text(if chain == "ela" { "E…" } else { "0x…" }),
                    );
                    super::input_ring(ui, theme, &to_resp);
                    ui.add_space(10.0);
                    ui.label(RichText::new("Amount").size(12.0).color(theme.muted));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let amt_resp = ui.add(
                            egui::TextEdit::singleline(&mut app.state.wallet.send.amount)
                                .desired_width(ui.available_width() - 60.0)
                                .font(egui::FontId::proportional(17.0))
                                .hint_text("0.0"),
                        );
                        super::input_ring(ui, theme, &amt_resp);
                        ui.label(RichText::new(&symbol).size(14.0).family(icons::semibold()).color(theme.gold_ink));
                    });
                    if !app.state.wallet.send.status.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&app.state.wallet.send.status).size(12.0).color(LIKE));
                    }
                    ui.add_space(16.0);
                    let ready = !app.state.wallet.send.to.trim().is_empty() && !app.state.wallet.send.amount.trim().is_empty();
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Gold when ready; a dimmed gold (non-actionable) until both fields are filled.
                        let resp = if ready {
                            super::primary_button(ui, false, "Review")
                        } else {
                            super::push_button(ui, false, "Review", GOLD.gamma_multiply(0.4), GOLD.gamma_multiply(0.4), NAVY)
                        };
                        if ready && resp.clicked() {
                            // Early recipient pre-validation (== Android checkAddress /
                            // isElaAddress): catch a typo before the confirm step. On
                            // invalid, show an inline error and DON'T advance. The deeper
                            // validation in walletops::send still runs at send time.
                            let to = app.state.wallet.send.to.clone();
                            match crate::walletops::precheck_recipient(&chain, &to) {
                                Ok(()) => {
                                    app.state.wallet.send.status.clear();
                                    app.state.wallet.send.stage = SendStage::Review;
                                }
                                Err(e) => app.state.wallet.send.status = e,
                            }
                        }
                    });
                }
                SendStage::Review => {
                    let to = app.state.wallet.send.to.clone();
                    let amount = app.state.wallet.send.amount.clone();
                    review_row(ui, theme, "Amount", &format!("{amount} {symbol}"));
                    review_row(ui, theme, "To", &short_addr(&to));
                    review_row(ui, theme, "Network", chain_full(&chain));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Transfers are irreversible. Confirm the recipient and network.")
                            .size(11.0)
                            .color(theme.muted),
                    );
                    if !app.state.wallet.send.status.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&app.state.wallet.send.status).size(12.0).color(LIKE));
                    }
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::primary_button(ui, false, "Confirm & send").clicked() {
                            do_send = Some((chain.clone(), token.clone(), to.clone(), amount.clone()));
                        }
                        ui.add_space(8.0);
                        if super::outline_button(ui, theme, false, "Back").clicked() {
                            app.state.wallet.send.stage = SendStage::Edit;
                        }
                    });
                }
                SendStage::Sending => {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(RichText::new("Broadcasting…").size(15.0).color(theme.gold_ink));
                        ui.label(RichText::new("Signing and sending to the network.").size(11.0).color(theme.muted));
                    });
                    ui.add_space(10.0);
                }
                SendStage::Done => {
                    // The Done screen reflects on-chain confirmation, not just broadcast.
                    // For EVM the receipt poll flips `conf` Pending → success/failed; the
                    // ELA mainchain has no receipt lookup so it stays "pending" (shown as
                    // a plain "Transaction sent", no spinner — it doesn't poll).
                    let hash = app.state.wallet.send.tx_hash.clone();
                    let conf = app.state.wallet.send.conf.clone();
                    let polling = app.state.wallet.send.polling;
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        match conf.as_str() {
                            "success" => {
                                ui.label(RichText::new(icons::CHECK_CIRCLE).size(44.0).color(theme.good));
                                ui.add_space(6.0);
                                ui.label(RichText::new("Confirmed").size(16.0).family(icons::semibold()).color(theme.ink));
                                ui.add_space(4.0);
                                ui.label(RichText::new("Your transfer is confirmed on-chain.").size(11.0).color(theme.muted));
                            }
                            "failed" => {
                                ui.label(RichText::new(icons::ERROR).size(44.0).color(LIKE));
                                ui.add_space(6.0);
                                ui.label(RichText::new("Failed on-chain").size(16.0).family(icons::semibold()).color(theme.ink));
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("The transaction reverted — gas was spent but the funds were NOT sent. Re-check the recipient and try again.")
                                        .size(11.0)
                                        .color(theme.muted),
                                );
                            }
                            _ if polling => {
                                // EVM, still confirming: spinner + broadcast copy.
                                ui.spinner();
                                ui.add_space(6.0);
                                ui.label(RichText::new("Broadcast").size(16.0).family(icons::semibold()).color(theme.ink));
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new("Sent to the network — confirming on-chain (usually a few seconds)…")
                                        .size(11.0)
                                        .color(theme.muted),
                                );
                            }
                            _ => {
                                // ELA mainchain (no poll) or the poll budget ran out.
                                ui.label(RichText::new(icons::CHECK_CIRCLE).size(44.0).color(theme.good));
                                ui.add_space(6.0);
                                ui.label(RichText::new("Transaction sent").size(16.0).family(icons::semibold()).color(theme.ink));
                            }
                        }
                        ui.add_space(6.0);
                        if !hash.is_empty() {
                            ui.label(RichText::new(short_addr(&hash)).size(12.0).color(theme.muted).monospace());
                        }
                    });
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::primary_button(ui, false, "Done").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });

    if let Some((c, tok, to, amount)) = do_send {
        app.state.wallet.send.stage = SendStage::Sending;
        app.state.wallet.send.status.clear();
        app.wallet_send(c, tok, to, amount, String::new());
    }
    if close {
        app.state.wallet.send = SendForm::default();
    }
}

// ── tip sheet ─────────────────────────────────────────────────────────────────
//
// A desktop port of the Android `TipSheet`. Tip by IDENTITY: the recipient's DID
// is resolved (`app.resolve_tip` → social::refresh_contact_addresses) to their
// PUBLISHED receive address per chain — never a typed/guessed address. The send
// goes through the SAME `app.tip_send` → walletops::send path (tagged kind:"tip"),
// behind the egui confirm screen (the spend gate). Floats over any tab.
pub fn tip_sheet(app: &mut App, ctx: &egui::Context, theme: &Theme) {
    if !app.state.tip.open {
        return;
    }
    let name = app.state.tip.name.clone();
    let stage = app.state.tip.stage.clone();
    let chains = app.state.tip.chains.clone();
    let chain = app.state.tip.chain.clone();
    let tokens = app.state.tip.tokens.clone();
    let token = app.state.tip.token.clone();
    // The symbol shown next to the amount = the selected ERC-20, else the chain's coin.
    let tip_sym = token
        .as_ref()
        .and_then(|t| t.get("symbol").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| chains.iter().find(|(k, _)| *k == chain).map(|(_, s)| s.clone()).unwrap_or_default());
    let screen = ctx.screen_rect();

    let mut close = false;
    let mut select_chain: Option<String> = None;
    let mut select_token: Option<Option<Value>> = None; // Some(None) = native, Some(Some(t)) = ERC-20
    let mut go_review = false;
    let mut go_edit = false;
    let mut do_send = false;
    let mut retry = false;

    egui::Window::new("wallet-tip")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, crate::app::sheet_rise(ctx)))
        .frame(theme.sheet())
        .show(ctx, |ui| {
            ui.set_width((screen.width() - 64.0).min(440.0));
            theme.sheet_handle(ui);

            // header
            ui.horizontal(|ui| {
                ui.label(RichText::new(icons::PAID).size(22.0).color(theme.gold_ink));
                ui.add_space(8.0);
                ui.label(
                    RichText::new(format!("Tip {name}"))
                        .size(20.0)
                        .family(icons::semibold())
                        .color(theme.ink),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // No close button while broadcasting (don't abandon a live send).
                    if stage != TipStage::Sending
                        && super::icon_button(ui, theme, icons::CLOSE, 18.0, theme.muted).clicked()
                    {
                        close = true;
                    }
                });
            });
            ui.add_space(2.0);
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, theme.glass_border));
            ui.add_space(10.0);
            ui.label(
                RichText::new("Sent by identity — Hey finds their address. You never need it.")
                    .size(11.0)
                    .color(theme.muted),
            );
            ui.add_space(12.0);

            match stage {
                TipStage::Resolving => {
                    ui.add_space(8.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(RichText::new("Finding their address…").size(14.0).color(theme.gold_ink));
                    });
                    ui.add_space(8.0);
                }
                TipStage::Edit => {
                    if chains.is_empty() {
                        // No published receive address → CANNOT tip (do not send).
                        ui.label(
                            RichText::new(format!("We don't have {name}'s wallet address yet."))
                                .size(15.0)
                                .family(icons::semibold())
                                .color(theme.ink),
                        );
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "There's no server — their address arrives with their profile over the \
                                 network. If you follow them it usually syncs within moments. Try again in a bit.",
                            )
                            .size(12.0)
                            .color(theme.muted),
                        );
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if super::primary_button(ui, false, &format!("{}  Try again", icons::REFRESH)).clicked() {
                                retry = true;
                            }
                            ui.add_space(8.0);
                            if super::outline_button(ui, theme, false, "Close").clicked() {
                                close = true;
                            }
                        });
                        ui.add_space(8.0);
                    } else {
                        // Chain picker.
                        ui.label(RichText::new("Chain").size(12.0).color(theme.muted));
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for (k, _sym) in &chains {
                                let on = *k == chain;
                                let label = match k.as_str() {
                                    "ela" => "ELA · main chain",
                                    "esc" => "ESC",
                                    other => other,
                                };
                                if chip(ui, theme, label, on).clicked() {
                                    select_chain = Some(k.clone());
                                }
                            }
                        });
                        // Asset picker — ERC-20s on ESC (native + each held token).
                        if chain == "esc" && tokens.len() > 1 {
                            ui.add_space(10.0);
                            ui.label(RichText::new("Asset").size(12.0).color(theme.muted));
                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                for t in &tokens {
                                    let native = t.get("native").and_then(Value::as_bool).unwrap_or(false);
                                    let sym = t.get("symbol").and_then(Value::as_str).unwrap_or("");
                                    let on = if native {
                                        token.is_none()
                                    } else {
                                        token.as_ref().and_then(|x| x.get("contract").and_then(Value::as_str))
                                            == t.get("contract").and_then(Value::as_str)
                                    };
                                    if chip(ui, theme, sym, on).clicked() {
                                        select_token = Some(if native { None } else { Some(t.clone()) });
                                    }
                                }
                            });
                        }
                        ui.add_space(12.0);
                        ui.label(RichText::new("Amount").size(12.0).color(theme.muted));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let amt_resp = ui.add(
                                egui::TextEdit::singleline(&mut app.state.tip.amount)
                                    .desired_width(ui.available_width() - 60.0)
                                    .font(egui::FontId::proportional(17.0))
                                    .hint_text("0.0"),
                            );
                            super::input_ring(ui, theme, &amt_resp);
                            ui.label(RichText::new(&tip_sym).size(14.0).family(icons::semibold()).color(theme.gold_ink));
                        });
                        if !app.state.tip.status.is_empty() {
                            ui.add_space(8.0);
                            ui.label(RichText::new(&app.state.tip.status).size(12.0).color(LIKE));
                        }
                        ui.add_space(16.0);
                        let ready = !app.state.tip.amount.trim().is_empty();
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let resp = if ready {
                                super::primary_button(ui, false, "Review & tip")
                            } else {
                                super::push_button(ui, false, "Review & tip", GOLD.gamma_multiply(0.4), GOLD.gamma_multiply(0.4), NAVY)
                            };
                            if ready && resp.clicked() {
                                go_review = true;
                            }
                        });
                    }
                }
                TipStage::Review => {
                    let amount = app.state.tip.amount.clone();
                    review_row(ui, theme, "Amount", &format!("{amount} {tip_sym}"));
                    review_row(ui, theme, "To", &name);
                    review_row(ui, theme, "Network", chain_full(&chain));
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Transfers are irreversible. Confirm the recipient and network.")
                            .size(11.0)
                            .color(theme.muted),
                    );
                    if !app.state.tip.status.is_empty() {
                        ui.add_space(8.0);
                        ui.label(RichText::new(&app.state.tip.status).size(12.0).color(LIKE));
                    }
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::primary_button(ui, false, "Confirm & tip").clicked() {
                            do_send = true;
                        }
                        ui.add_space(8.0);
                        if super::outline_button(ui, theme, false, "Back").clicked() {
                            go_edit = true;
                        }
                    });
                }
                TipStage::Sending => {
                    ui.add_space(10.0);
                    ui.vertical_centered(|ui| {
                        ui.spinner();
                        ui.add_space(8.0);
                        ui.label(RichText::new("Broadcasting…").size(15.0).color(theme.gold_ink));
                        ui.label(RichText::new("Signing and sending to the network.").size(11.0).color(theme.muted));
                    });
                    ui.add_space(10.0);
                }
                TipStage::Done => {
                    let hash = app.state.tip.tx_hash.clone();
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(icons::CHECK_CIRCLE).size(44.0).color(theme.good));
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("Tipped {name}"))
                                .size(16.0)
                                .family(icons::semibold())
                                .color(theme.ink),
                        );
                        ui.add_space(6.0);
                        if !hash.is_empty() {
                            ui.label(RichText::new(short_addr(&hash)).size(12.0).color(theme.muted).monospace());
                        }
                    });
                    ui.add_space(16.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if super::primary_button(ui, false, "Done").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });

    // ── apply intents (no app.state borrow live) ────────────────────────────────
    if let Some(k) = select_chain {
        if k != app.state.tip.chain {
            app.state.tip.chain = k.clone();
            app.state.tip.token = None;
            app.state.tip.tokens.clear();
            app.state.tip.symbol =
                app.state.tip.chains.iter().find(|(c, _)| *c == k).map(|(_, s)| s.clone()).unwrap_or_default();
            app.state.tip.status.clear();
            if k == "esc" {
                let did = app.state.tip.did.clone();
                app.load_tip_tokens(&did);
            }
        }
    }
    if let Some(t) = select_token {
        app.state.tip.token = t;
    }
    if go_review {
        // Validate the amount + that a resolved address exists for the chain BEFORE
        // the confirm step. The runtime send re-validates everything again.
        let amt = app.state.tip.amount.trim();
        if amt.parse::<f64>().map(|v| v <= 0.0).unwrap_or(true) {
            app.state.tip.status = "Enter an amount".into();
        } else if app.state.tip.addresses.get(&app.state.tip.chain).map(|s| s.trim().is_empty()).unwrap_or(true) {
            app.state.tip.status = "They haven't published an address for this chain".into();
        } else {
            app.state.tip.status.clear();
            app.state.tip.stage = TipStage::Review;
        }
    }
    if go_edit {
        app.state.tip.stage = TipStage::Edit;
    }
    if do_send {
        // Resolve the recipient address from the by-identity lookup — NEVER typed.
        let chain = app.state.tip.chain.clone();
        let to = app.state.tip.addresses.get(&chain).cloned().unwrap_or_default();
        if to.trim().is_empty() {
            // Defensive: refuse to send without a resolved address.
            app.state.tip.status = "Can't tip — recipient hasn't published a receive address".into();
        } else {
            let token = app.state.tip.token.clone();
            let amount = app.state.tip.amount.clone();
            let to_did = app.state.tip.did.clone();
            app.state.tip.stage = TipStage::Sending;
            app.state.tip.status.clear();
            app.tip_send(chain, token, to, amount, to_did);
        }
    }
    if retry {
        let did = app.state.tip.did.clone();
        app.state.tip.stage = TipStage::Resolving;
        app.state.tip.status.clear();
        app.resolve_tip(&did);
    }
    if close {
        app.state.tip = TipForm::default();
    }
}

/// A selectable pill chip (chain / asset picker) matching the Android tip chips.
fn chip(ui: &mut egui::Ui, theme: &Theme, label: &str, on: bool) -> egui::Response {
    let (bg, border, fg) = if on {
        (GOLD.gamma_multiply(0.22), theme.gold_ink, theme.gold_ink)
    } else {
        (theme.hover, theme.glass_border, theme.ink)
    };
    egui::Frame::none()
        .fill(bg)
        .stroke(Stroke::new(1.0, border))
        .rounding(20.0)
        .inner_margin(egui::Margin::symmetric(14.0, 8.0))
        .show(ui, |ui| {
            let weight = if on { icons::semibold() } else { egui::FontFamily::Proportional };
            ui.label(RichText::new(label).size(13.0).family(weight).color(fg));
        })
        .response
        .interact(Sense::click())
}

fn review_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).size(13.0).color(theme.muted));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(14.0).family(icons::semibold()).color(theme.ink));
        });
    });
    ui.add_space(6.0);
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// A subhead section label (13pt SemiBold, muted) above a grouped card.
fn section_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.label(RichText::new(text).size(13.0).family(icons::semibold()).color(theme.muted));
}

/// An inset hairline between rows inside a grouped card (14px in from each edge).
fn row_divider(ui: &mut egui::Ui, theme: &Theme) {
    let r = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().hline(
        (r.left() + 14.0)..=(r.right() - 14.0),
        y,
        Stroke::new(1.0, theme.glass_border),
    );
}

/// (symbol, balance_decimal) for a chain's native coin from a balance bundle.
fn native_of(chain: &str, bal: Option<&Value>) -> (String, String) {
    match bal {
        None => (native_symbol(chain).to_string(), "—".to_string()),
        Some(b) if chain == "ela" => (
            "ELA".to_string(),
            b.get("ela").and_then(Value::as_str).unwrap_or("0").to_string(),
        ),
        Some(b) => b
            .get("tokens")
            .and_then(Value::as_array)
            .and_then(|ts| ts.iter().find(|t| t.get("native").and_then(Value::as_bool).unwrap_or(false)))
            .map(|t| {
                (
                    t.get("symbol").and_then(Value::as_str).unwrap_or("").to_string(),
                    t.get("balance").and_then(Value::as_str).unwrap_or("0").to_string(),
                )
            })
            .unwrap_or_else(|| (native_symbol(chain).to_string(), "0".to_string())),
    }
}

/// The curated non-native tokens from an EVM balance bundle.
fn tokens_of(bal: Option<&Value>) -> Vec<Value> {
    bal.and_then(|b| b.get("tokens"))
        .and_then(Value::as_array)
        .map(|ts| {
            ts.iter()
                .filter(|t| !t.get("native").and_then(Value::as_bool).unwrap_or(false))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn short_addr(a: &str) -> String {
    let chars: Vec<char> = a.chars().collect();
    if chars.len() <= 16 {
        a.to_string()
    } else {
        let start: String = chars[..8].iter().collect();
        let end: String = chars[chars.len() - 6..].iter().collect();
        format!("{start}…{end}")
    }
}

fn chain_short(key: &str) -> &str {
    match key {
        "esc" => "ESC",
        "eid" => "EID",
        "ethereum" => "Ethereum",
        "ela" => "ELA",
        other => other,
    }
}

fn chain_full(key: &str) -> &str {
    match key {
        "esc" => "Elastos Smart Chain",
        "eid" => "Elastos Identity Chain",
        "ethereum" => "Ethereum",
        "ela" => "Elastos Mainchain",
        other => other,
    }
}

fn native_symbol(key: &str) -> &str {
    match key {
        "ethereum" => "ETH",
        _ => "ELA", // ESC native + ELA mainchain
    }
}

fn qr_texture(ctx: &egui::Context, link: &str) -> Option<TextureHandle> {
    crate::qr::qr_texture(ctx, link)
}
