//! Theme system.
//!
//! All chrome colors live on a `Palette` struct; the active palette is
//! held in a `OnceLock<Palette>` accessed via `palette()`. Layout
//! constants stay as `pub const` because they don't change with theme.
//!
//! The palette can be loaded from a YAML file (terminal-theme schema:
//! 4 base colors + 16 ANSI colors), or selected from the bundled set:
//! `Midnight`, `Gruvbox Dark`, `Vercel`, `Dracula`.

use std::path::Path;
use std::sync::atomic::{AtomicPtr, Ordering};

// ── Layout constants (theme-independent) ─────────────────────────
pub const FONT_PX: f32 = 18.0;
pub const SIDE_PADDING: f32 = 30.0;
pub const TOP_PADDING: f32 = 124.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;
pub const STATUS_BAR_GAP: f32 = 8.0;
pub const STATUS_BAR_SIDE_PADDING: f32 = 12.0;
pub const ATLAS_SIZE: u32 = 4096;

// ─────────────────────────────────────────────────────────────────
//  Palette: every color the renderer reads.
// ─────────────────────────────────────────────────────────────────

#[allow(dead_code)] // some palette slots are reserved for future UI surfaces
#[derive(Clone, Debug)]
pub struct Palette {
    pub name: &'static str,

    // Surfaces
    pub bg: wgpu::Color,
    pub titlebar_top: [f32; 4],
    pub titlebar_bottom: [f32; 4],
    pub toolbar_top: [f32; 4],
    pub toolbar_bottom: [f32; 4],
    pub content_top: [f32; 4],
    pub content_bottom: [f32; 4],

    // Text
    pub text_primary: [f32; 4],
    pub text_secondary: [f32; 4],
    pub text_muted: [f32; 4],
    pub text_accent: [f32; 4],
    pub text_bright: [f32; 4],

    // Semantic accents
    pub accent_green: [f32; 4],
    pub accent_green_dim: [f32; 4],
    pub accent_yellow: [f32; 4],
    pub accent_yellow_dim: [f32; 4],
    pub accent_red: [f32; 4],
    pub accent_red_dim: [f32; 4],
    pub accent_blue: [f32; 4],
    pub accent_blue_dim: [f32; 4],
    pub accent_gray: [f32; 4],
    pub accent_gray_dim: [f32; 4],
    pub accent_purple: [f32; 4],
    pub accent_purple_dim: [f32; 4],
    pub accent_pink: [f32; 4],
    pub accent_pink_dim: [f32; 4],

    // Selection
    pub row_selected: [f32; 4],
    pub row_selected_bottom: [f32; 4],
    pub row_selected_border: [f32; 4],
    pub selection_accent_bar: [f32; 4],

    // Section header backgrounds
    pub section_staged_bg_top: [f32; 4],
    pub section_staged_bg_bottom: [f32; 4],
    pub section_staged_border: [f32; 4],
    pub section_unstaged_bg_top: [f32; 4],
    pub section_unstaged_bg_bottom: [f32; 4],
    pub section_unstaged_border: [f32; 4],
    pub section_untracked_bg_top: [f32; 4],
    pub section_untracked_bg_bottom: [f32; 4],
    pub section_untracked_border: [f32; 4],

    // Diff tints
    pub diff_add_bg_top: [f32; 4],
    pub diff_add_bg_bottom: [f32; 4],
    pub diff_remove_bg_top: [f32; 4],
    pub diff_remove_bg_bottom: [f32; 4],
    pub diff_hunk_bg_top: [f32; 4],
    pub diff_hunk_bg_bottom: [f32; 4],
    pub diff_hunk_border: [f32; 4],
    pub diff_meta_bg_top: [f32; 4],
    pub diff_meta_bg_bottom: [f32; 4],
    pub diff_file_header_bg_top: [f32; 4],
    pub diff_file_header_bg_bottom: [f32; 4],
    pub diff_file_header_border: [f32; 4],

    // Chrome
    pub toolbar_separator: [f32; 4],
    pub divider: [f32; 4],

    // Modals
    pub modal_bg_top: [f32; 4],
    pub modal_bg_bottom: [f32; 4],
    pub modal_border: [f32; 4],
    pub modal_danger_bg_top: [f32; 4],
    pub modal_danger_bg_bottom: [f32; 4],
    pub modal_danger_border: [f32; 4],

    pub tooltip_bg_top: [f32; 4],
    pub tooltip_bg_bottom: [f32; 4],
    pub tooltip_border: [f32; 4],
    pub tooltip_text: [f32; 4],

    // Branch indicator
    pub branch_current_badge: [f32; 4],
    pub branch_chip_bg_top: [f32; 4],
    pub branch_chip_bg_bottom: [f32; 4],

    // Diff line-number gutter — slightly darker than bg
    pub gutter_top: [f32; 4],
    pub gutter_bottom: [f32; 4],

    // Status bar (top, bottom, border, text)
    pub status_neutral: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]),
    pub status_success: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]),
    pub status_error: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]),
    pub status_prompt: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]),

    // Document line styles
    pub line_normal: [f32; 4],
    pub line_dim: [f32; 4],
    pub line_header: [f32; 4],
    pub line_selected: [f32; 4],
    pub line_diff_add: [f32; 4],
    pub line_diff_remove: [f32; 4],
    pub line_diff_hunk: [f32; 4],

    // Git status badges
    pub badge_modified: [f32; 4],
    pub badge_added: [f32; 4],
    pub badge_deleted: [f32; 4],
    pub badge_renamed: [f32; 4],
    pub badge_untracked: [f32; 4],
    pub badge_copied: [f32; 4],
}

// ── Active palette ───────────────────────────────────────────────
//
// Held as an `AtomicPtr<Palette>` so themes can be swapped at runtime
// from the settings modal. Each swap leaks one Palette (~600 bytes);
// since switches are user-driven (a click), the leak is bounded.
// `palette()` returns `&'static Palette` so existing field-access
// callsites compile unchanged.
static ACTIVE: AtomicPtr<Palette> = AtomicPtr::new(std::ptr::null_mut());

/// Returns the active palette. Initialises to `Palette::midnight()`
/// on first call if `set_palette` hasn't been called yet.
pub fn palette() -> &'static Palette {
    let ptr = ACTIVE.load(Ordering::Acquire);
    if !ptr.is_null() {
        // SAFETY: Pointer is only ever set via `Box::into_raw`, and the
        // backing allocation is intentionally leaked for `'static`.
        return unsafe { &*ptr };
    }
    let leaked = Box::into_raw(Box::new(Palette::midnight()));
    match ACTIVE.compare_exchange(
        std::ptr::null_mut(),
        leaked,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(_) => unsafe { &*leaked },
        Err(other) => {
            // Lost the race — drop our allocation, use the other thread's.
            unsafe { drop(Box::from_raw(leaked)) };
            unsafe { &*other }
        }
    }
}

/// Replace the active palette. Returns true (the previous palette is
/// intentionally leaked since outstanding `&'static Palette` references
/// may still hold it).
pub fn set_palette(p: Palette) -> bool {
    let leaked = Box::into_raw(Box::new(p));
    let _old = ACTIVE.swap(leaked, Ordering::AcqRel);
    true
}

/// Resolve a theme name to a bundled palette.
pub fn bundled(name: &str) -> Option<Palette> {
    match name.to_ascii_lowercase().as_str() {
        "midnight" | "default" | "dark" => Some(Palette::midnight()),
        "gruvbox" | "gruvbox dark" | "gruvbox-dark" => Some(Palette::gruvbox_dark()),
        "vercel" | "vercel dark" | "vercel-dark" => Some(Palette::vercel_dark()),
        "dracula" => Some(Palette::dracula()),
        _ => None,
    }
}

/// Names of every bundled theme.
pub fn bundled_names() -> &'static [&'static str] {
    &["Midnight", "Gruvbox Dark", "Vercel", "Dracula"]
}

/// Load a YAML theme from disk and return its palette.
pub fn load_yaml_file(path: &Path) -> Result<Palette, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path.display(), e))?;
    load_yaml_str(&text)
}

/// Parse a YAML theme string and return its palette.
pub fn load_yaml_str(text: &str) -> Result<Palette, String> {
    let raw = parse_theme_yaml(text)?;
    Ok(derive_palette(&raw))
}

// ─────────────────────────────────────────────────────────────────
//  Theme YAML format
//
//  Schema (subset we accept):
//    name: ...
//    accent: '#hex'
//    background: '#hex'
//    foreground: '#hex'
//    details: 'darker' | 'lighter'
//    terminal_colors:
//      normal:  { black, red, green, yellow, blue, magenta, cyan, white }
//      bright:  { black, red, green, yellow, blue, magenta, cyan, white }
// ─────────────────────────────────────────────────────────────────

#[derive(Default, Debug)]
struct AnsiColors {
    black: [f32; 3],
    red: [f32; 3],
    green: [f32; 3],
    yellow: [f32; 3],
    blue: [f32; 3],
    magenta: [f32; 3],
    cyan: [f32; 3],
    white: [f32; 3],
}

#[derive(Default, Debug)]
struct ThemeYaml {
    name: String,
    accent: [f32; 3],
    background: [f32; 3],
    foreground: [f32; 3],
    /// true = "darker" (chrome details darker than bg, typical for
    /// medium-dark themes); false = "lighter".
    details_darker: bool,
    normal: AnsiColors,
    bright: AnsiColors,
}

fn parse_theme_yaml(text: &str) -> Result<ThemeYaml, String> {
    let mut t = ThemeYaml {
        details_darker: true,
        ..Default::default()
    };
    let mut section_path: Vec<String> = Vec::new();
    let mut section_indents: Vec<usize> = Vec::new();

    for (lineno, raw) in text.lines().enumerate() {
        let stripped = raw.trim_end();
        // Skip blanks and YAML comments
        let trimmed = stripped.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = stripped.len() - trimmed.len();

        // Pop sections whose indent is >= this line's indent
        while let Some(&top) = section_indents.last() {
            if indent <= top {
                section_indents.pop();
                section_path.pop();
            } else {
                break;
            }
        }

        let (key, val) = trimmed
            .split_once(':')
            .ok_or_else(|| format!("line {}: expected 'key: value'", lineno + 1))?;
        let key = key.trim();
        let val = val.trim();

        if val.is_empty() {
            section_path.push(key.to_string());
            section_indents.push(indent);
            continue;
        }

        let unquoted = val
            .trim_start_matches(['\'', '"'])
            .trim_end_matches(['\'', '"']);
        let qualified = if section_path.is_empty() {
            key.to_string()
        } else {
            format!("{}.{}", section_path.join("."), key)
        };

        apply_yaml_value(&mut t, &qualified, unquoted, lineno + 1)?;
    }

    Ok(t)
}

fn apply_yaml_value(t: &mut ThemeYaml, key: &str, val: &str, lineno: usize) -> Result<(), String> {
    match key {
        "name" => t.name = val.to_string(),
        "accent" => t.accent = parse_hex(val).map_err(|e| format!("line {}: {}", lineno, e))?,
        "background" => {
            t.background = parse_hex(val).map_err(|e| format!("line {}: {}", lineno, e))?
        }
        "foreground" => {
            t.foreground = parse_hex(val).map_err(|e| format!("line {}: {}", lineno, e))?
        }
        "details" => t.details_darker = val.eq_ignore_ascii_case("darker"),
        // terminal_colors.{normal,bright}.{black|red|green|...}
        k if k.starts_with("terminal_colors.normal.") => {
            apply_ansi(&mut t.normal, &k["terminal_colors.normal.".len()..], val, lineno)?;
        }
        k if k.starts_with("terminal_colors.bright.") => {
            apply_ansi(&mut t.bright, &k["terminal_colors.bright.".len()..], val, lineno)?;
        }
        // unknown keys are ignored (forward-compatible)
        _ => {}
    }
    Ok(())
}

fn apply_ansi(a: &mut AnsiColors, slot: &str, val: &str, lineno: usize) -> Result<(), String> {
    let c = parse_hex(val).map_err(|e| format!("line {}: {}", lineno, e))?;
    match slot {
        "black" => a.black = c,
        "red" => a.red = c,
        "green" => a.green = c,
        "yellow" => a.yellow = c,
        "blue" => a.blue = c,
        "magenta" => a.magenta = c,
        "cyan" => a.cyan = c,
        "white" => a.white = c,
        _ => {} // ignore unknown
    }
    Ok(())
}

/// Parse a hex color (`#rrggbb`, `#rgb`, or `0xRRGGBB`) into linear-ish
/// 0..1 RGB. We don't apply gamma — wgit treats inputs as already in
/// the working color space.
fn parse_hex(s: &str) -> Result<[f32; 3], String> {
    let s = s.trim();
    let body = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")))
        .unwrap_or(s);
    let (r, g, b) = match body.len() {
        6 => (
            u8::from_str_radix(&body[0..2], 16),
            u8::from_str_radix(&body[2..4], 16),
            u8::from_str_radix(&body[4..6], 16),
        ),
        3 => {
            let exp = |c: char| -> Result<u8, _> {
                u8::from_str_radix(&format!("{c}{c}"), 16)
            };
            let cs: Vec<char> = body.chars().collect();
            (exp(cs[0]), exp(cs[1]), exp(cs[2]))
        }
        _ => return Err(format!("invalid hex color {:?}", s)),
    };
    let r = r.map_err(|e| format!("invalid hex color {:?}: {}", s, e))?;
    let g = g.map_err(|e| format!("invalid hex color {:?}: {}", s, e))?;
    let b = b.map_err(|e| format!("invalid hex color {:?}: {}", s, e))?;
    Ok([
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
    ])
}

// ─────────────────────────────────────────────────────────────────
//  Color math helpers
// ─────────────────────────────────────────────────────────────────

#[inline]
fn rgba(c: [f32; 3], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

#[inline]
fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[inline]
fn shift(c: [f32; 3], by: f32) -> [f32; 3] {
    [
        (c[0] + by).clamp(0.0, 1.0),
        (c[1] + by).clamp(0.0, 1.0),
        (c[2] + by).clamp(0.0, 1.0),
    ]
}

/// "Toward fg" — useful for deriving softer accent dim variants.
#[inline]
fn toward(c: [f32; 3], target: [f32; 3], t: f32) -> [f32; 3] {
    lerp3(c, target, t)
}

// ─────────────────────────────────────────────────────────────────
//  Derive a full Palette from the 4 base + 16 ANSI colors
// ─────────────────────────────────────────────────────────────────

fn derive_palette(y: &ThemeYaml) -> Palette {
    let bg = y.background;
    let fg = y.foreground;

    // Direction of chrome shift: "darker" themes have chrome panels
    // slightly darker than bg; "lighter" themes have them slightly
    // lighter.
    let dir: f32 = if y.details_darker { -1.0 } else { 1.0 };

    let titlebar_top = shift(bg, 0.018 * -dir); // titlebars feel lifted
    let titlebar_bottom = shift(bg, 0.000);
    let toolbar_top = shift(bg, 0.008 * -dir);
    let toolbar_bottom = shift(bg, 0.010 * dir);
    let content_top = shift(bg, 0.005 * dir);
    let content_bottom = shift(bg, 0.018 * dir);

    let modal_top = shift(bg, 0.020 * -dir);
    let modal_bot = shift(bg, 0.018 * dir);
    let modal_border = toward(fg, bg, 0.65);

    // Text
    let text_primary = fg;
    let text_secondary = lerp3(fg, bg, 0.30);
    let text_muted = lerp3(fg, bg, 0.55);
    let text_accent = y.accent;

    // ANSI semantic accents (use bright for vividness, normal for dim)
    let ag = y.bright.green;
    let ay = y.bright.yellow;
    let ar = y.bright.red;
    let ab = y.bright.blue;
    let amag = y.bright.magenta;
    let acyan = y.bright.cyan;
    let agray = y.normal.white;

    let dim = |c: [f32; 3]| rgba(lerp3(c, bg, 0.30), 0.70);

    // Sections — saturated tints. We render with alpha so a section
    // band painted over `bg` reads as ≈ alpha × tint_amount of the
    // accent. Bumped from the previous ~11% effective tint to ~50% so
    // the source themes (gruvbox, dracula, etc.) don't look washed out.
    let mk_section = |c: [f32; 3]| {
        let t = lerp3(bg, c, 0.55);
        let b = lerp3(bg, c, 0.42);
        let bd = lerp3(c, bg, 0.20);
        (
            [t[0], t[1], t[2], 0.85],
            [b[0], b[1], b[2], 0.78],
            [bd[0], bd[1], bd[2], 0.65],
        )
    };

    let (sst, ssb, ssbd) = mk_section(ag);
    let (ust, usb, usbd) = mk_section(ay);
    let (utt, utb, utbd) = mk_section(agray);

    // Diff add/remove tints — visible but not so loud they fight the
    // syntax-coloured text on top.
    let mk_diff = |c: [f32; 3], top_a: f32, bot_a: f32| {
        let t = lerp3(bg, c, 0.45);
        let b = lerp3(bg, c, 0.36);
        ([t[0], t[1], t[2], top_a], [b[0], b[1], b[2], bot_a])
    };
    let (dat, dab) = mk_diff(ag, 0.55, 0.48);
    let (drt, drb) = mk_diff(ar, 0.55, 0.48);

    // Hunk header — definitively a header band, not a faint suggestion.
    let dh_t = lerp3(bg, ab, 0.40);
    let dh_b = lerp3(bg, ab, 0.28);
    let dh_bd = lerp3(ab, bg, 0.20);
    let dm_t = lerp3(bg, ab, 0.18);
    let dm_b = lerp3(bg, ab, 0.10);

    // Diff file header — clearly lifted, with a foreground stroke.
    let dfh_t = lerp3(bg, fg, 0.18);
    let dfh_b = lerp3(bg, fg, 0.10);
    let dfh_bd = lerp3(fg, bg, 0.40);

    // Selection — solid lift so the active row pops.
    let row_top = rgba(lerp3(bg, fg, 0.20), 0.85);
    let row_bot = rgba(lerp3(bg, fg, 0.14), 0.78);
    let row_bd = rgba(lerp3(fg, bg, 0.30), 0.65);
    let sel_bar = rgba(y.accent, 1.0);

    // Branch chip — visibly chipped from the chrome.
    let chip_top = rgba(lerp3(bg, fg, 0.20), 0.92);
    let chip_bot = rgba(lerp3(bg, fg, 0.14), 0.88);

    // Modal danger — red-tinted bg
    let md_top = lerp3(bg, ar, 0.30);
    let md_bot = lerp3(bg, ar, 0.18);
    let md_border = toward(ar, bg, 0.10);

    // Diff line-number gutter — slightly darker than bg so it reads
    // as a visual rail without competing with the diff content.
    let gut_t = shift(bg, -0.04 * -dir);
    let gut_b = shift(bg, -0.06 * -dir);

    // Status bar variants
    let status_n_t = rgba(modal_top, 0.92);
    let status_n_b = rgba(shift(modal_bot, 0.0), 0.94);
    let status_n_bd = rgba(lerp3(fg, bg, 0.70), 0.50);
    let status_n_text = rgba(lerp3(fg, bg, 0.10), 1.0);

    let status_s_t = rgba(lerp3(bg, ag, 0.18), 0.92);
    let status_s_b = rgba(lerp3(bg, ag, 0.12), 0.94);
    let status_s_bd = rgba(lerp3(ag, bg, 0.45), 0.55);
    let status_s_text = rgba(lerp3(ag, [1.0, 1.0, 1.0], 0.45), 1.0);

    let status_e_t = rgba(lerp3(bg, ar, 0.20), 0.92);
    let status_e_b = rgba(lerp3(bg, ar, 0.13), 0.94);
    let status_e_bd = rgba(lerp3(ar, bg, 0.30), 0.62);
    let status_e_text = rgba(lerp3(ar, [1.0, 1.0, 1.0], 0.55), 1.0);

    let status_p_t = rgba(lerp3(bg, ay, 0.20), 0.92);
    let status_p_b = rgba(lerp3(bg, ay, 0.13), 0.94);
    let status_p_bd = rgba(lerp3(ay, bg, 0.40), 0.58);
    let status_p_text = rgba(lerp3(ay, [1.0, 1.0, 1.0], 0.55), 1.0);

    Palette {
        name: leak_name(&y.name),

        bg: wgpu::Color {
            r: bg[0] as f64,
            g: bg[1] as f64,
            b: bg[2] as f64,
            a: 1.0,
        },
        titlebar_top: rgba(titlebar_top, 1.0),
        titlebar_bottom: rgba(titlebar_bottom, 1.0),
        toolbar_top: rgba(toolbar_top, 1.0),
        toolbar_bottom: rgba(toolbar_bottom, 1.0),
        content_top: rgba(content_top, 1.0),
        content_bottom: rgba(content_bottom, 1.0),

        text_primary: rgba(text_primary, 1.0),
        text_secondary: rgba(text_secondary, 1.0),
        text_muted: rgba(text_muted, 1.0),
        text_accent: rgba(text_accent, 1.0),
        text_bright: [1.0, 1.0, 1.0, 1.0],

        accent_green: rgba(ag, 1.0),
        accent_green_dim: dim(ag),
        accent_yellow: rgba(ay, 1.0),
        accent_yellow_dim: dim(ay),
        accent_red: rgba(ar, 1.0),
        accent_red_dim: dim(ar),
        accent_blue: rgba(ab, 1.0),
        accent_blue_dim: dim(ab),
        accent_gray: rgba(agray, 1.0),
        accent_gray_dim: dim(agray),
        accent_purple: rgba(amag, 1.0),
        accent_purple_dim: dim(amag),
        accent_pink: rgba(lerp3(amag, ar, 0.30), 1.0),
        accent_pink_dim: dim(lerp3(amag, ar, 0.30)),

        row_selected: row_top,
        row_selected_bottom: row_bot,
        row_selected_border: row_bd,
        selection_accent_bar: sel_bar,

        section_staged_bg_top: sst,
        section_staged_bg_bottom: ssb,
        section_staged_border: ssbd,
        section_unstaged_bg_top: ust,
        section_unstaged_bg_bottom: usb,
        section_unstaged_border: usbd,
        section_untracked_bg_top: utt,
        section_untracked_bg_bottom: utb,
        section_untracked_border: utbd,

        diff_add_bg_top: dat,
        diff_add_bg_bottom: dab,
        diff_remove_bg_top: drt,
        diff_remove_bg_bottom: drb,
        diff_hunk_bg_top: rgba(dh_t, 0.55),
        diff_hunk_bg_bottom: rgba(dh_b, 0.45),
        diff_hunk_border: rgba(dh_bd, 0.35),
        diff_meta_bg_top: rgba(dm_t, 0.30),
        diff_meta_bg_bottom: rgba(dm_b, 0.25),
        diff_file_header_bg_top: rgba(dfh_t, 0.85),
        diff_file_header_bg_bottom: rgba(dfh_b, 0.78),
        diff_file_header_border: rgba(dfh_bd, 0.50),

        toolbar_separator: rgba(lerp3(fg, bg, 0.65), 0.32),
        divider: rgba(lerp3(fg, bg, 0.70), 0.55),

        modal_bg_top: rgba(modal_top, 1.0),
        modal_bg_bottom: rgba(modal_bot, 1.0),
        modal_border: rgba(modal_border, 0.55),
        modal_danger_bg_top: rgba(md_top, 1.0),
        modal_danger_bg_bottom: rgba(md_bot, 1.0),
        modal_danger_border: rgba(md_border, 0.60),

        tooltip_bg_top: rgba(lerp3(bg, [0.0, 0.0, 0.0], 0.88), 1.0),
        tooltip_bg_bottom: rgba(lerp3(bg, [0.0, 0.0, 0.0], 0.92), 1.0),
        tooltip_border: rgba([1.0, 1.0, 1.0], 0.12),
        tooltip_text: rgba([0.96, 0.97, 0.99], 1.0),

        branch_current_badge: rgba(ag, 1.0),
        branch_chip_bg_top: chip_top,
        branch_chip_bg_bottom: chip_bot,

        gutter_top: rgba(gut_t, 1.0),
        gutter_bottom: rgba(gut_b, 1.0),

        status_neutral: (status_n_t, status_n_b, status_n_bd, status_n_text),
        status_success: (status_s_t, status_s_b, status_s_bd, status_s_text),
        status_error: (status_e_t, status_e_b, status_e_bd, status_e_text),
        status_prompt: (status_p_t, status_p_b, status_p_bd, status_p_text),

        line_normal: rgba(lerp3(fg, bg, 0.08), 1.0),
        line_dim: rgba(text_muted, 1.0),
        line_header: rgba(lerp3(fg, ab, 0.22), 1.0),
        line_selected: [1.0, 1.0, 1.0, 1.0],
        line_diff_add: rgba(ag, 1.0),
        line_diff_remove: rgba(ar, 1.0),
        line_diff_hunk: rgba(lerp3(fg, ab, 0.30), 1.0),

        badge_modified: rgba(ay, 1.0),
        badge_added: rgba(ag, 1.0),
        badge_deleted: rgba(ar, 1.0),
        badge_renamed: rgba(amag, 1.0),
        badge_untracked: rgba(agray, 1.0),
        badge_copied: rgba(acyan, 1.0),
    }
}

/// We hold theme names as `&'static str` because the Palette is stored
/// in a `OnceLock`. For loaded YAML themes we leak the parsed name into
/// `'static`. There's at most one leak per process (one set_palette
/// call), so this is bounded.
fn leak_name(s: &str) -> &'static str {
    if s.is_empty() {
        return "Custom";
    }
    Box::leak(s.to_string().into_boxed_str())
}

// ─────────────────────────────────────────────────────────────────
//  Bundled themes — defined inline and run through the same
//  derivation as user-provided themes, so the styling stays uniform.
// ─────────────────────────────────────────────────────────────────

const MIDNIGHT_YAML: &str = r#"
name: Midnight
accent: '#7daea3'
background: '#1c1f24'
foreground: '#e6e7eb'
details: 'lighter'
terminal_colors:
  normal:
    black:   '#1c1f24'
    red:     '#cc6666'
    green:   '#a3be8c'
    yellow:  '#d8a657'
    blue:    '#7eafce'
    magenta: '#b294bb'
    cyan:    '#7daea3'
    white:   '#a0a3a8'
  bright:
    black:   '#5c6370'
    red:     '#ea6962'
    green:   '#6ec690'
    yellow:  '#d8a657'
    blue:    '#7daea3'
    magenta: '#d3869b'
    cyan:    '#7daea3'
    white:   '#e6e7eb'
"#;

const GRUVBOX_DARK_YAML: &str = r#"
name: Gruvbox Dark
accent: '#fe8019'
background: '#282828'
foreground: '#ebdbb2'
details: 'darker'
terminal_colors:
  normal:
    black:   '#282828'
    red:     '#cc241d'
    green:   '#98971a'
    yellow:  '#d79921'
    blue:    '#458588'
    magenta: '#b16286'
    cyan:    '#689d6a'
    white:   '#a89984'
  bright:
    black:   '#928374'
    red:     '#fb4934'
    green:   '#b8bb26'
    yellow:  '#fabd2f'
    blue:    '#83a598'
    magenta: '#d3869b'
    cyan:    '#8ec07c'
    white:   '#ebdbb2'
"#;

const VERCEL_DARK_YAML: &str = r#"
name: Vercel
accent: '#0070f3'
background: '#000000'
foreground: '#ededed'
details: 'lighter'
terminal_colors:
  normal:
    black:   '#000000'
    red:     '#ee0000'
    green:   '#50e3c2'
    yellow:  '#f5a623'
    blue:    '#0070f3'
    magenta: '#f81ce5'
    cyan:    '#79ffe1'
    white:   '#a0a0a0'
  bright:
    black:   '#666666'
    red:     '#ff4444'
    green:   '#7cffd9'
    yellow:  '#ffcb6b'
    blue:    '#3291ff'
    magenta: '#ff7eea'
    cyan:    '#aaffe5'
    white:   '#ededed'
"#;

const DRACULA_YAML: &str = r#"
name: Dracula
accent: '#ff79c6'
background: '#282a36'
foreground: '#f8f8f2'
details: 'darker'
terminal_colors:
  normal:
    black:   '#21222c'
    red:     '#ff5555'
    green:   '#50fa7b'
    yellow:  '#f1fa8c'
    blue:    '#bd93f9'
    magenta: '#ff79c6'
    cyan:    '#8be9fd'
    white:   '#bfbfbf'
  bright:
    black:   '#6272a4'
    red:     '#ff6e6e'
    green:   '#69ff94'
    yellow:  '#ffffa5'
    blue:    '#d6acff'
    magenta: '#ff92df'
    cyan:    '#a4ffff'
    white:   '#f8f8f2'
"#;

impl Palette {
    pub fn midnight() -> Self {
        load_yaml_str(MIDNIGHT_YAML).expect("bundled Midnight YAML")
    }
    pub fn gruvbox_dark() -> Self {
        load_yaml_str(GRUVBOX_DARK_YAML).expect("bundled Gruvbox YAML")
    }
    pub fn vercel_dark() -> Self {
        load_yaml_str(VERCEL_DARK_YAML).expect("bundled Vercel YAML")
    }
    pub fn dracula() -> Self {
        load_yaml_str(DRACULA_YAML).expect("bundled Dracula YAML")
    }
}

// ─────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────

pub fn badge_color_for_status(xy: &str) -> [f32; 4] {
    let chars: Vec<char> = xy.chars().collect();
    let x = chars.first().copied().unwrap_or(' ');
    let y = chars.get(1).copied().unwrap_or(' ');
    let c = if x != ' ' && x != '?' { x } else { y };
    let p = palette();
    match c {
        'M' => p.badge_modified,
        'A' => p.badge_added,
        'D' => p.badge_deleted,
        'R' => p.badge_renamed,
        'C' => p.badge_copied,
        '?' => p.badge_untracked,
        _ => p.text_muted,
    }
}

pub fn badge_char_for_status(xy: &str) -> char {
    let chars: Vec<char> = xy.chars().collect();
    let x = chars.first().copied().unwrap_or(' ');
    let y = chars.get(1).copied().unwrap_or(' ');
    if x == '?' && y == '?' {
        return '?';
    }
    if x != ' ' && x != '?' {
        return x;
    }
    y
}
