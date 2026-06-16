//! Typography + iconography — "Aurora". Inter (an SF substitute) for text in
//! Regular/Medium/SemiBold/Bold + Inter Display Bold for large titles, with
//! Material Icons as a glyph fallback. `setup()` installs them into egui's font
//! stack; the consts below are the exact Material codepoints the UI uses — write
//! them in a RichText and they render as real icons.

/// Install Inter as the proportional UI font (Regular by default) and Material
/// Icons as a glyph fallback (so an icon const in normal text renders as the
/// icon). Named families expose each weight + a tight display cut for large
/// titles. Keeps egui's default emoji/CJK fallbacks behind them.
pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts
            .font_data
            .insert(name.to_owned(), egui::FontData::from_static(bytes));
    };
    add("Inter", include_bytes!("../assets/Inter-Regular.ttf"));
    add("Inter-Medium", include_bytes!("../assets/Inter-Medium.ttf"));
    add("Inter-SemiBold", include_bytes!("../assets/Inter-SemiBold.ttf"));
    add("Inter-Bold", include_bytes!("../assets/Inter-Bold.ttf"));
    add("InterDisplay", include_bytes!("../assets/InterDisplay-Bold.ttf"));
    add("MaterialIcons", include_bytes!("../assets/MaterialIcons-Regular.ttf"));

    // Body default = Inter Regular (reads cleaner than Medium at small sizes),
    // Material Icons as glyph fallback, then egui's emoji/CJK defaults.
    let prop = fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default();
    prop.insert(0, "Inter".to_owned());
    prop.insert(1, "MaterialIcons".to_owned());

    let fam = |v: Vec<&str>| v.into_iter().map(str::to_owned).collect::<Vec<_>>();
    fonts
        .families
        .insert(egui::FontFamily::Name("regular".into()), fam(vec!["Inter", "MaterialIcons"]));
    fonts
        .families
        .insert(egui::FontFamily::Name("medium".into()), fam(vec!["Inter-Medium", "MaterialIcons"]));
    fonts
        .families
        .insert(egui::FontFamily::Name("semibold".into()), fam(vec!["Inter-SemiBold", "MaterialIcons"]));
    fonts
        .families
        .insert(egui::FontFamily::Name("bold".into()), fam(vec!["Inter-Bold", "MaterialIcons"]));
    fonts
        .families
        .insert(egui::FontFamily::Name("display".into()), fam(vec!["InterDisplay", "Inter-Bold", "MaterialIcons"]));

    ctx.set_fonts(fonts);
}

/// Inter Medium (subtle emphasis — chips, meta).
pub fn medium() -> egui::FontFamily {
    egui::FontFamily::Name("medium".into())
}

/// Inter SemiBold (row names, card headers, segmented selected, buttons).
pub fn semibold() -> egui::FontFamily {
    egui::FontFamily::Name("semibold".into())
}

/// Inter Display Bold (large titles ≥ 28pt only — tight optical cut).
pub fn display() -> egui::FontFamily {
    egui::FontFamily::Name("display".into())
}

// ── Material Icons codepoints (filled set) ────────────────────────────────────
// Navigation / tabs
pub const FORUM: &str = "\u{e0bf}";
pub const DYNAMIC_FEED: &str = "\u{ea14}";
pub const NOTIFICATIONS: &str = "\u{e7f4}";
pub const ACCOUNT_CIRCLE: &str = "\u{e853}";

// Actions
pub const FAVORITE: &str = "\u{e87d}";
pub const FAVORITE_BORDER: &str = "\u{e87e}";
pub const CHAT_BUBBLE_OUTLINE: &str = "\u{e0cb}";
pub const ADD: &str = "\u{e145}";
pub const MORE_VERT: &str = "\u{e5d4}";
pub const ARROW_BACK: &str = "\u{e5c4}";
pub const SEARCH: &str = "\u{e8b6}";
pub const CLOSE: &str = "\u{e5cd}";
pub const ATTACH_FILE: &str = "\u{e226}";
pub const SEND: &str = "\u{e163}";
pub const EDIT: &str = "\u{e3c9}";
pub const CONTENT_COPY: &str = "\u{e14d}";
pub const CHEVRON_RIGHT: &str = "\u{e5cc}";
pub const DOWNLOAD: &str = "\u{f090}";
pub const BLOCK: &str = "\u{e14b}";
pub const NOTIFICATIONS_OFF: &str = "\u{e7f6}";

// People / contacts
pub const PERSON_ADD: &str = "\u{e7fe}";
pub const GROUP_ADD: &str = "\u{e7f0}";
pub const PERSON: &str = "\u{e7fd}";

// QR / identity
pub const QR_CODE_2: &str = "\u{e00a}";
pub const BADGE: &str = "\u{ea67}";
pub const KEY: &str = "\u{e73c}";

// Status / settings / about
pub const SETTINGS: &str = "\u{e8b8}";
pub const HUB: &str = "\u{e9f4}";
pub const LOCK: &str = "\u{e897}";
pub const SHIELD: &str = "\u{e9e0}";
pub const VERIFIED_USER: &str = "\u{e8e8}";
pub const BOLT: &str = "\u{ea0b}";
pub const INFO: &str = "\u{e88e}";
pub const PUBLIC: &str = "\u{e80b}";
pub const CLOUD_OFF: &str = "\u{e2c1}";
pub const SWAP_HORIZ: &str = "\u{e8d4}";
pub const SMARTPHONE: &str = "\u{e32c}";

// Media
pub const PHOTO_CAMERA: &str = "\u{e412}";
pub const ADD_A_PHOTO: &str = "\u{e439}";
pub const ADD_PHOTO_ALTERNATE: &str = "\u{e43e}";
pub const PLAY_CIRCLE: &str = "\u{e1c4}";
pub const PLAY_ARROW: &str = "\u{e037}";
pub const DESCRIPTION: &str = "\u{e873}";

// Selection
pub const CHECK_CIRCLE: &str = "\u{e86c}";
pub const RADIO_UNCHECKED: &str = "\u{e836}";
pub const ERROR: &str = "\u{e000}";
pub const VISIBILITY: &str = "\u{e8f4}";
pub const VISIBILITY_OFF: &str = "\u{e8f5}";

// Wallet
pub const ACCOUNT_BALANCE_WALLET: &str = "\u{e850}";
// `monetization_on` — a filled coin (the desktop's "tip / paid" glyph; the classic
// MaterialIcons font has no `paid`, so we use the coin that ships in the font).
pub const PAID: &str = "\u{e263}";
pub const ARROW_UPWARD: &str = "\u{e5d8}";
pub const ARROW_DOWNWARD: &str = "\u{e5db}";
pub const RECEIPT_LONG: &str = "\u{ef6e}";

// Misc
pub const REFRESH: &str = "\u{e5d5}";
pub const LINK: &str = "\u{e157}";
pub const CHECK: &str = "\u{e5ca}";
pub const COMPUTER: &str = "\u{e30a}";
