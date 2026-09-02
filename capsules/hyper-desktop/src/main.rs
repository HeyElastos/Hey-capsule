//! Hyper Desktop, as a capsule.
//!
//! The shell is a transcription of the desktop app's `views/shell.rs`: an
//! aurora backdrop, a 76pt icon spine, and separate floating glass planes with
//! real gaps between them. Same seven destinations, same order, same tokens.
//!
//! What this is NOT is a compile of that app. Freya has no web backend and the
//! Skia fork ships no wasm build, so `src/ui` and `src/views` cannot cross —
//! they are Freya vocabulary against a GPU canvas. What crosses is the design
//! system and the layout grammar, which is most of what makes it look like
//! Hyper. See `styles.css`; every token there has a counterpart in the
//! desktop's `ui/theme.rs`.
//!
//! The ENGINE does cross, which is the part worth knowing. `hey-core` has a
//! wasm build, and Chat runs on the very same `api::dms` the phone and desktop
//! apps use — same sealed-sender wire, same on-disk layout, same ratchet. What
//! changes underneath is only the transport: natively it opens iroh sockets,
//! and here it routes `provider_call` over the runtime's HTTP, because a
//! browser tab has no UDP socket. The DM layer neither knows nor cares.
//!
//! Auth is ElastOS Home (`?home_token=`). Ed25519, X25519 and ML-KEM live in
//! this capsule. See `runtime.rs`, which is the only file that knows the
//! runtime's wire shape.

use hey_core::ctx::{init, CapsuleCtx};
use leptos::prelude::*;

mod activity;
mod calls;
mod chat;
mod media;
mod prefs;
mod profile;
mod runtime;
mod social;
mod wallet;
mod workspace;

/// A did, shortened for reading.
///
/// CHARS AND NOT BYTES. `&s[..8]` panics the moment the string is not ASCII,
/// and while a did:key always is, the same helper gets pointed at display names
/// that are not — which is how the desktop app earned this rule the hard way.
/// One implementation so there is one place for that to be true.
pub fn shorten_did(did: &str) -> String {
    if did.is_empty() {
        return "-".into();
    }
    let s = did.strip_prefix("did:key:").unwrap_or(did);
    let n = s.chars().count();
    if n <= 14 {
        return s.to_string();
    }
    let head: String = s.chars().take(8).collect();
    let tail: String = s.chars().skip(n - 4).collect();
    format!("{head}\u{2026}{tail}")
}

/// Stable hue from a DID so two people never share an avatar tint by accident.
pub fn avatar_hue(did: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in did.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    h % 360
}

pub fn tint_css(did: &str) -> String {
    format!("--tint:hsl({} 42% 38%)", avatar_hue(did))
}


/// Relative time, to the coarsest unit that is still true.
///
/// "3h" and not "3 hours 12 minutes ago": in a list the exact figure is noise,
/// and the reason to show it at all is ordering.
pub fn ago(ts_ms: i64) -> String {
    if ts_ms <= 0 {
        return String::new();
    }
    let now = js_sys::Date::now() as i64;
    let secs = ((now - ts_ms) / 1000).max(0);
    match secs {
        s if s < 60 => "now".to_string(),
        s if s < 3600 => format!("{}m", s / 60),
        s if s < 86_400 => format!("{}h", s / 3600),
        s => format!("{}d", s / 86_400),
    }
}

/// A destination in the spine. Same seven as the desktop, in the same order —
/// the order is muscle memory for anyone moving between the two.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Chat,
    Social,
    Calls,
    Wallet,
    Workspace,
    Activity,
    Profile,
}

impl Tab {
    const ORDER: [Tab; 7] = [
        Tab::Chat,
        Tab::Social,
        Tab::Calls,
        Tab::Wallet,
        Tab::Workspace,
        Tab::Activity,
        Tab::Profile,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Chat => "Messages",
            Tab::Social => "Social",
            Tab::Calls => "Calls",
            Tab::Wallet => "Wallet",
            Tab::Workspace => "Workspaces",
            Tab::Activity => "Activity",
            // "You", not "Profile": the desktop calls it that because the page
            // is the person, not a settings section about them.
            Tab::Profile => "You",
        }
    }

    /// Lucide glyphs, matching the desktop's `Ico` mapping one for one.
    fn glyph(self) -> &'static str {
        match self {
            Tab::Chat => "messages-square",
            // Social is a PHOTO feed, not a syndication feed — an RSS glyph
            // describes the wrong product.
            Tab::Social => "images",
            Tab::Calls => "phone",
            Tab::Wallet => "wallet",
            Tab::Workspace => "layout-grid",
            Tab::Activity => "bell",
            Tab::Profile => "user",
        }
    }

    /// Whether this destination shows the right-hand rail. Matches the desktop:
    /// Calls, Workspace and Activity use their full width.
    fn has_rail(self) -> bool {
        matches!(
            self,
            Tab::Chat | Tab::Social | Tab::Profile
        )
    }
}

#[component]
fn Aurora() -> impl IntoView {
    view! {
        <div class="aurora" aria-hidden="true">
            <i></i>
            <i></i>
            <i></i>
        </div>
    }
}

#[component]
fn Spine(tab: RwSignal<Tab>) -> impl IntoView {
    view! {
        <nav class="plane spine">
            <div class="mark">"\u{26A1}"</div>
            {Tab::ORDER
                .into_iter()
                .map(|t| {
                    let current = move || tab.get() == t;
                    view! {
                        <button
                            title=t.title()
                            aria-current=move || if current() { "page" } else { "false" }
                            on:click=move |_| tab.set(t)
                        >
                            <Icon name=t.glyph() />
                        </button>
                    }
                })
                .collect_view()}
        </nav>
    }
}

/// The icon set, inline.
///
/// Inline and not an icon font or sprite sheet: a capsule is served from its own
/// mount path with no CDN reachable, and a missing glyph in the spine is a
/// destination the user cannot identify.
#[component]
fn Icon(name: &'static str) -> impl IntoView {
    let d = match name {
        "messages-square" => "M14 9a2 2 0 0 1-2 2H6l-4 4V4a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2z M18 9h2a2 2 0 0 1 2 2v11l-4-4h-6a2 2 0 0 1-2-2v-1",
        "images" => "M18 22H4a2 2 0 0 1-2-2V6 M8 14l2.5-3 2 2.5L16 9l4 5 M22 4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2z",
        "phone" => "M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z",
        "wallet" => "M19 7V4a1 1 0 0 0-1-1H5a2 2 0 0 0 0 4h15a1 1 0 0 1 1 1v4h-3a2 2 0 0 0 0 4h3a1 1 0 0 0 1-1v-2 M3 5v14a2 2 0 0 0 2 2h15a1 1 0 0 0 1-1v-4",
        "layout-grid" => "M3 3h7v7H3z M14 3h7v7h-7z M14 14h7v7h-7z M3 14h7v7H3z",
        "bell" => "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9 M10.3 21a1.94 1.94 0 0 0 3.4 0",
        "search" => "M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16z M21 21l-4.35-4.35",
        "plus" => "M12 5v14 M5 12h14",
        _ => "M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2 M12 3a4 4 0 1 0 0 8 4 4 0 0 0 0-8z",
    };
    view! {
        <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
        >
            <path d=d></path>
        </svg>
    }
}

/// A pane: a plane with a frosted bar its body travels under.
#[component]
fn Pane(tab: Tab, children: Children) -> impl IntoView {
    view! {
        <section class="plane" style="flex:1">
            <header class="bar">
                <h1>{tab.title()}</h1>
                <div class="spring"></div>
                <button class="icon-btn" title="Search">
                    <Icon name="search" />
                </button>
                <button class="icon-btn" title="New">
                    <Icon name="plus" />
                </button>
            </header>
            <div class="body">{children()}</div>
        </section>
    }
}

#[component]
fn Rail(session: ReadSignal<runtime::Session>) -> impl IntoView {
    view! {
        <aside class="plane rail">
            <header class="bar">
                <h1 style="font-size:var(--ty-lead)">"Network"</h1>
            </header>
            <div class="body">
                <h3>"Runtime"</h3>
                <div class="row">
                    <span>"Session"</span>
                    <span>{move || session.get().state}</span>
                </div>
                <div class="row">
                    <span>"Identity"</span>
                    <span>
                        {move || {
                            let s = session.get();
                            if s.did.is_empty() {
                                "held by ElastOS".to_string()
                            } else if !s.name.is_empty() {
                                s.name
                            } else {
                                s.did.chars().skip(8).take(10).collect::<String>()
                            }
                        }}
                    </span>
                </div>
                <div class="card">
                    <h2>"Keys stay in this capsule"</h2>
                    <p>
                        "ElastOS Home authenticates you. This capsule holds the social and PQ keys, and ElastOS Carrier carries the mesh."
                    </p>
                </div>
            </div>
        </aside>
    }
}

#[component]
fn App() -> impl IntoView {
    let tab = RwSignal::new(Tab::Chat);
    let (session, set_session) = signal(runtime::Session::booting());

    // Home token, then capsule keys. See runtime::boot.
    runtime::boot(set_session);
    crate::prefs::apply_skin();

    let rail_on = RwSignal::new(crate::prefs::rail_pinned());
    provide_context(rail_on);

    view! {
        <Aurora />
        <div class="frame">
            <Spine tab=tab />
            {move || {
                match tab.get() {
                    // The two built destinations render their own panes, because
                    // each wants a different shape: Chat is a list beside a
                    // thread, Social is a single lane.
                    Tab::Chat => view! { <chat::Chat /> }.into_any(),
                    Tab::Social => view! { <social::Social /> }.into_any(),
                    Tab::Profile => view! { <profile::Profile /> }.into_any(),
                    Tab::Activity => view! { <activity::Activity /> }.into_any(),
                    Tab::Calls => view! { <calls::Calls /> }.into_any(),
                    Tab::Wallet => view! { <wallet::Wallet /> }.into_any(),
                    Tab::Workspace => view! { <workspace::Workspace /> }.into_any(),
                }
            }}
            {move || {
                (tab.get().has_rail() && rail_on.get()).then(|| view! { <Rail session=session /> })
            }}
        </div>
    }
}

/// Per-capsule identity for the shared engine.
///
/// WITHOUT THIS THE APP IS A BLANK PAGE, and not in a way that looks like a
/// crash. `hey_core::ctx` holds the context in a `OnceCell` whose accessor
/// `.expect()`s, and with the workspace's `lto = true` the optimiser sees a
/// static that nothing in the program ever writes, folds the read to `None`,
/// and concludes every engine call diverges. Everything downstream — the whole
/// view tree, Leptos's DOM machinery, all of hey-core's transport — becomes
/// provably unreachable and is deleted. The build succeeds, the wasm comes out
/// a fifth of its real size, and the page renders nothing.
///
/// Own namespace and own storage keys: the session is per-capsule, so this
/// signs in independently of hey-chat and hey-social even though it is the same
/// person and the same DID underneath.
///
/// The capability list is what this capsule actually uses, and no more — it has
/// to stay inside what capsule.json grants:
///   peer     Carrier iroh-gossip transport, which carries DMs
///   content  the feed's post bodies and media
///   did      ElastOS device DID / nickname
/// Wallet is the Runtime app at /apps/wallet/, not a provider_call here.
const HYPER_DESKTOP_CTX: CapsuleCtx = CapsuleCtx {
    capsule_id: "hyper-desktop",
    private_namespace: "HyperDesktop",
    session_key: "hyper-desktop-session",
    welcomed_key: "hyper-desktop-welcomed",
    session_redeemed_key: "hyper-desktop-redeemed",
    home_launch_token_key: "hyper-desktop-home-token",
    runtime_token_key: "hyper-desktop-runtime-token",
    token_store_key: "hyper-desktop-capability-tokens",
    route_mode_key: "hyper-desktop-storage-route-mode",
    boot_capabilities: &[
        ("elastos://peer/*", "message"),
        ("elastos://content/*", "read"),
        ("elastos://did/*", "read"),
    ],
};

fn main() {
    console_error_panic_hook::set_once();
    // MUST precede every engine call, including the ones App spawns at mount.
    init(HYPER_DESKTOP_CTX);
    leptos::mount::mount_to_body(App);
}
