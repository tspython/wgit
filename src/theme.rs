// ── Layout constants ──────────────────────────────────────────────
pub const FONT_PX: f32 = 18.0;
pub const SIDE_PADDING: f32 = 30.0;
pub const TOP_PADDING: f32 = 124.0;
pub const STATUS_BAR_HEIGHT: f32 = 28.0;
pub const STATUS_BAR_GAP: f32 = 8.0;
pub const STATUS_BAR_SIDE_PADDING: f32 = 12.0;
pub const ATLAS_SIZE: u32 = 4096;

// ── Surface / background colors ──────────────────────────────────
pub const COLOR_BG: wgpu::Color = wgpu::Color {
    r: 0.065,
    g: 0.068,
    b: 0.082,
    a: 1.0,
};

/// Titlebar background gradient
pub const COLOR_TITLEBAR_TOP: [f32; 4] = [0.10, 0.11, 0.15, 1.0];
pub const COLOR_TITLEBAR_BOTTOM: [f32; 4] = [0.08, 0.09, 0.13, 1.0];

/// Toolbar background gradient
pub const COLOR_TOOLBAR_TOP: [f32; 4] = [0.12, 0.14, 0.20, 1.0];
pub const COLOR_TOOLBAR_BOTTOM: [f32; 4] = [0.09, 0.11, 0.16, 1.0];

/// Content panel background gradient
pub const COLOR_CONTENT_TOP: [f32; 4] = [0.075, 0.080, 0.105, 1.0];
pub const COLOR_CONTENT_BOTTOM: [f32; 4] = [0.065, 0.070, 0.095, 1.0];

// ── Text hierarchy ───────────────────────────────────────────────
pub const TEXT_PRIMARY: [f32; 4] = [0.93, 0.95, 0.98, 1.0];
pub const TEXT_SECONDARY: [f32; 4] = [0.72, 0.76, 0.84, 1.0];
pub const TEXT_MUTED: [f32; 4] = [0.48, 0.52, 0.60, 1.0];
pub const TEXT_ACCENT: [f32; 4] = [0.55, 0.70, 1.0, 1.0];
pub const TEXT_BRIGHT: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

// ── Semantic accent colors ───────────────────────────────────────
/// Green: staged, additions, success
pub const ACCENT_GREEN: [f32; 4] = [0.40, 0.82, 0.52, 1.0];
pub const ACCENT_GREEN_DIM: [f32; 4] = [0.30, 0.62, 0.40, 0.70];

/// Yellow/orange: unstaged, modified, warnings
pub const ACCENT_YELLOW: [f32; 4] = [0.92, 0.78, 0.38, 1.0];
pub const ACCENT_YELLOW_DIM: [f32; 4] = [0.72, 0.60, 0.28, 0.70];

/// Red: danger, deletions, errors, untracked
pub const ACCENT_RED: [f32; 4] = [0.95, 0.45, 0.42, 1.0];
pub const ACCENT_RED_DIM: [f32; 4] = [0.75, 0.35, 0.32, 0.70];

/// Blue: info, links, focused, selected
pub const ACCENT_BLUE: [f32; 4] = [0.45, 0.62, 1.0, 1.0];
pub const ACCENT_BLUE_DIM: [f32; 4] = [0.35, 0.48, 0.80, 0.70];

/// Gray: neutral, untracked
pub const ACCENT_GRAY: [f32; 4] = [0.55, 0.58, 0.64, 1.0];
pub const ACCENT_GRAY_DIM: [f32; 4] = [0.40, 0.43, 0.48, 0.70];

/// Purple: remote ops (push/pull/fetch)
pub const ACCENT_PURPLE: [f32; 4] = [0.68, 0.52, 0.98, 1.0];
pub const ACCENT_PURPLE_DIM: [f32; 4] = [0.48, 0.36, 0.72, 0.70];

// ── Selection / focus ────────────────────────────────────────────
pub const COLOR_ROW_SELECTED: [f32; 4] = [0.20, 0.30, 0.58, 0.35];
pub const COLOR_ROW_SELECTED_BOTTOM: [f32; 4] = [0.16, 0.24, 0.48, 0.30];
pub const COLOR_ROW_SELECTED_BORDER: [f32; 4] = [0.45, 0.60, 0.98, 0.50];
pub const COLOR_SELECTION_ACCENT_BAR: [f32; 4] = [0.45, 0.65, 1.0, 0.90];

// ── Section header backgrounds ───────────────────────────────────
/// Staged section: green-tinted
pub const SECTION_STAGED_BG_TOP: [f32; 4] = [0.12, 0.20, 0.15, 0.60];
pub const SECTION_STAGED_BG_BOTTOM: [f32; 4] = [0.09, 0.16, 0.12, 0.50];
pub const SECTION_STAGED_BORDER: [f32; 4] = [0.30, 0.65, 0.40, 0.40];

/// Unstaged section: yellow-tinted
pub const SECTION_UNSTAGED_BG_TOP: [f32; 4] = [0.20, 0.18, 0.10, 0.60];
pub const SECTION_UNSTAGED_BG_BOTTOM: [f32; 4] = [0.16, 0.14, 0.08, 0.50];
pub const SECTION_UNSTAGED_BORDER: [f32; 4] = [0.65, 0.55, 0.30, 0.40];

/// Untracked section: gray-tinted
pub const SECTION_UNTRACKED_BG_TOP: [f32; 4] = [0.14, 0.14, 0.16, 0.60];
pub const SECTION_UNTRACKED_BG_BOTTOM: [f32; 4] = [0.11, 0.11, 0.13, 0.50];
pub const SECTION_UNTRACKED_BORDER: [f32; 4] = [0.45, 0.46, 0.50, 0.40];

// ── Diff background tints ────────────────────────────────────────
pub const DIFF_ADD_BG_TOP: [f32; 4] = [0.12, 0.22, 0.14, 0.40];
pub const DIFF_ADD_BG_BOTTOM: [f32; 4] = [0.10, 0.18, 0.12, 0.35];
pub const DIFF_REMOVE_BG_TOP: [f32; 4] = [0.24, 0.12, 0.12, 0.40];
pub const DIFF_REMOVE_BG_BOTTOM: [f32; 4] = [0.20, 0.10, 0.10, 0.35];
pub const DIFF_HUNK_BG_TOP: [f32; 4] = [0.14, 0.18, 0.28, 0.50];
pub const DIFF_HUNK_BG_BOTTOM: [f32; 4] = [0.11, 0.14, 0.22, 0.45];
pub const DIFF_HUNK_BORDER: [f32; 4] = [0.35, 0.48, 0.78, 0.35];
pub const DIFF_META_BG_TOP: [f32; 4] = [0.10, 0.12, 0.18, 0.30];
pub const DIFF_META_BG_BOTTOM: [f32; 4] = [0.08, 0.10, 0.15, 0.25];

/// Diff file header (the prominent bar above each file's diff)
pub const DIFF_FILE_HEADER_BG_TOP: [f32; 4] = [0.16, 0.20, 0.30, 0.80];
pub const DIFF_FILE_HEADER_BG_BOTTOM: [f32; 4] = [0.12, 0.15, 0.24, 0.75];
pub const DIFF_FILE_HEADER_BORDER: [f32; 4] = [0.38, 0.50, 0.80, 0.50];

// ── Toolbar button groups ────────────────────────────────────────
pub const TOOLBAR_SEPARATOR: [f32; 4] = [0.30, 0.33, 0.40, 0.30];

// ── Dividers / borders ───────────────────────────────────────────
pub const DIVIDER_COLOR: [f32; 4] = [0.22, 0.25, 0.32, 0.50];

// ── Modal overlay ────────────────────────────────────────────────
pub const MODAL_BG_TOP: [f32; 4] = [0.12, 0.14, 0.20, 0.97];
pub const MODAL_BG_BOTTOM: [f32; 4] = [0.08, 0.10, 0.16, 0.97];
pub const MODAL_BORDER: [f32; 4] = [0.35, 0.45, 0.70, 0.60];

pub const MODAL_DANGER_BG_TOP: [f32; 4] = [0.20, 0.12, 0.12, 0.97];
pub const MODAL_DANGER_BG_BOTTOM: [f32; 4] = [0.14, 0.08, 0.08, 0.97];
pub const MODAL_DANGER_BORDER: [f32; 4] = [0.80, 0.35, 0.35, 0.60];

// ── Status bar per-kind ──────────────────────────────────────────
pub const STATUS_NEUTRAL: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]) = (
    [0.12, 0.15, 0.22, 0.88],
    [0.09, 0.11, 0.17, 0.90],
    [0.30, 0.42, 0.65, 0.50],
    [0.82, 0.88, 0.96, 1.0],
);

pub const STATUS_SUCCESS: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]) = (
    [0.12, 0.22, 0.16, 0.90],
    [0.09, 0.17, 0.12, 0.92],
    [0.28, 0.65, 0.40, 0.55],
    [0.82, 0.96, 0.88, 1.0],
);

pub const STATUS_ERROR: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]) = (
    [0.28, 0.14, 0.14, 0.92],
    [0.20, 0.10, 0.10, 0.94],
    [0.78, 0.36, 0.36, 0.65],
    [1.0, 0.90, 0.90, 1.0],
);

pub const STATUS_PROMPT: ([f32; 4], [f32; 4], [f32; 4], [f32; 4]) = (
    [0.22, 0.18, 0.10, 0.92],
    [0.17, 0.14, 0.07, 0.94],
    [0.78, 0.64, 0.30, 0.60],
    [1.0, 0.94, 0.82, 1.0],
);

// ── Line style colors (used by Document/DocLine) ─────────────────
pub const LINE_NORMAL: [f32; 4] = [0.88, 0.90, 0.94, 1.0];
pub const LINE_DIM: [f32; 4] = [0.50, 0.54, 0.62, 1.0];
pub const LINE_HEADER: [f32; 4] = [0.78, 0.85, 1.0, 1.0];
pub const LINE_SELECTED: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const LINE_DIFF_ADD: [f32; 4] = [0.55, 0.90, 0.60, 1.0];
pub const LINE_DIFF_REMOVE: [f32; 4] = [0.95, 0.55, 0.55, 1.0];
pub const LINE_DIFF_HUNK: [f32; 4] = [0.55, 0.72, 1.0, 1.0];

// ── Git status badge colors ──────────────────────────────────────
pub const BADGE_MODIFIED: [f32; 4] = [0.92, 0.78, 0.38, 1.0]; // M - yellow
pub const BADGE_ADDED: [f32; 4] = [0.40, 0.82, 0.52, 1.0]; // A - green
pub const BADGE_DELETED: [f32; 4] = [0.95, 0.45, 0.42, 1.0]; // D - red
pub const BADGE_RENAMED: [f32; 4] = [0.68, 0.52, 0.98, 1.0]; // R - purple
pub const BADGE_UNTRACKED: [f32; 4] = [0.55, 0.58, 0.64, 1.0]; // ? - gray
pub const BADGE_COPIED: [f32; 4] = [0.45, 0.62, 1.0, 1.0]; // C - blue

// ── Helper: git status char → badge color ────────────────────────
pub fn badge_color_for_status(xy: &str) -> [f32; 4] {
    let chars: Vec<char> = xy.chars().collect();
    let x = chars.first().copied().unwrap_or(' ');
    let y = chars.get(1).copied().unwrap_or(' ');

    // Prefer the index (staged) status if present, else worktree status
    let c = if x != ' ' && x != '?' { x } else { y };

    match c {
        'M' => BADGE_MODIFIED,
        'A' => BADGE_ADDED,
        'D' => BADGE_DELETED,
        'R' => BADGE_RENAMED,
        'C' => BADGE_COPIED,
        '?' => BADGE_UNTRACKED,
        _ => TEXT_MUTED,
    }
}

/// Human-readable single-char badge for a git status code
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
