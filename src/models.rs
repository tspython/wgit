use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    Normal,
    Dim,
    Header,
    Selected,
    DiffAdd,
    DiffRemove,
    DiffHunk,
    DiffMeta,
    DiffFileHeader,
    SectionStaged,
    SectionUnstaged,
    SectionUntracked,
}

impl LineStyle {
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Normal => theme::palette().line_normal,
            Self::Dim => theme::palette().line_dim,
            Self::Header => theme::palette().line_header,
            Self::Selected => theme::palette().line_selected,
            Self::DiffAdd => theme::palette().line_diff_add,
            Self::DiffRemove => theme::palette().line_diff_remove,
            Self::DiffHunk => theme::palette().line_diff_hunk,
            Self::DiffMeta => theme::palette().line_dim,
            Self::DiffFileHeader => theme::palette().text_accent,
            Self::SectionStaged => theme::palette().accent_green,
            Self::SectionUnstaged => theme::palette().accent_yellow,
            Self::SectionUntracked => theme::palette().accent_gray,
        }
    }

    /// Whether this line style should get a full-width background tint
    pub fn has_background(self) -> bool {
        matches!(
            self,
            Self::DiffAdd
                | Self::DiffRemove
                | Self::DiffHunk
                | Self::DiffMeta
                | Self::DiffFileHeader
                | Self::SectionStaged
                | Self::SectionUnstaged
                | Self::SectionUntracked
        )
    }

    /// Background fill colors (top, bottom, border) for tinted lines
    pub fn background_colors(self) -> ([f32; 4], [f32; 4], [f32; 4]) {
        match self {
            Self::DiffAdd => (
                theme::palette().diff_add_bg_top,
                theme::palette().diff_add_bg_bottom,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffRemove => (
                theme::palette().diff_remove_bg_top,
                theme::palette().diff_remove_bg_bottom,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffHunk => (
                theme::palette().diff_hunk_bg_top,
                theme::palette().diff_hunk_bg_bottom,
                theme::palette().diff_hunk_border,
            ),
            Self::DiffMeta => (
                theme::palette().diff_meta_bg_top,
                theme::palette().diff_meta_bg_bottom,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffFileHeader => (
                theme::palette().diff_file_header_bg_top,
                theme::palette().diff_file_header_bg_bottom,
                theme::palette().diff_file_header_border,
            ),
            Self::SectionStaged => (
                theme::palette().section_staged_bg_top,
                theme::palette().section_staged_bg_bottom,
                theme::palette().section_staged_border,
            ),
            Self::SectionUnstaged => (
                theme::palette().section_unstaged_bg_top,
                theme::palette().section_unstaged_bg_bottom,
                theme::palette().section_unstaged_border,
            ),
            Self::SectionUntracked => (
                theme::palette().section_untracked_bg_top,
                theme::palette().section_untracked_bg_bottom,
                theme::palette().section_untracked_border,
            ),
            _ => ([0.0; 4], [0.0; 4], [0.0; 4]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarAction {
    RepoSwitch,
    Browse,
    BranchSwitch,
    Commit,
    Fetch,
    Pull,
    Push,
    Refresh,
    Stage,
    StageAll,
    Unstage,
    UnstageAll,
    Discard,
    Settings,
    Quit,
}

/// Groups for toolbar button visual separation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarGroup {
    Staging,
    GitOps,
    Danger,
    App,
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub enum WindowControlAction {
    Close,
    Minimize,
    Zoom,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolbarButton {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub action: ToolbarAction,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowControlButton {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub action: WindowControlAction,
}

#[derive(Clone, Copy, Debug)]
pub struct ColorSpan {
    pub start_col: usize,
    pub end_col: usize,
    pub color: [f32; 4],
}

/// Which pane currently has keyboard focus
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusPane {
    Files,
    Diff,
}

/// Line number info for diff lines (old line, new line)
#[derive(Clone, Copy, Debug, Default)]
pub struct DiffLineNumber {
    pub old: Option<u32>,
    pub new: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct DocLine {
    pub text: String,
    pub style: LineStyle,
    pub spans: Vec<ColorSpan>,
    /// Only set for diff document lines
    pub line_number: Option<DiffLineNumber>,
}

impl DocLine {
    pub fn new(text: impl Into<String>, style: LineStyle) -> Self {
        Self {
            text: text.into(),
            style,
            spans: Vec::new(),
            line_number: None,
        }
    }

    pub fn with_spans(mut self, spans: Vec<ColorSpan>) -> Self {
        self.spans = spans;
        self
    }

    pub fn with_line_number(mut self, ln: DiffLineNumber) -> Self {
        self.line_number = Some(ln);
        self
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    lines: Vec<DocLine>,
}

impl Document {
    pub fn from_lines(mut lines: Vec<DocLine>) -> Self {
        if lines.is_empty() {
            lines.push(DocLine::new("", LineStyle::Dim));
        }
        Self { lines }
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn line_text(&self, line_index: usize) -> &str {
        &self.lines[line_index].text
    }

    pub fn line_style(&self, line_index: usize) -> LineStyle {
        self.lines[line_index].style
    }

    pub fn line_spans(&self, line_index: usize) -> &[ColorSpan] {
        &self.lines[line_index].spans
    }

    pub fn line_number(&self, line_index: usize) -> Option<DiffLineNumber> {
        self.lines[line_index].line_number
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub x: f32,
    pub y: f32,
    pub color: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct VisualLine {
    pub y_top: f32,
    pub line_index: usize,
    pub style: LineStyle,
    pub glyphs: Vec<ShapedGlyph>,
    pub shaped: bool,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct GitViewMeta {
    pub files_start_line: usize,
    pub files_count: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub fill_top: [f32; 4],
    pub fill_bottom: [f32; 4],
    pub stroke: [f32; 4],
    pub text: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct ButtonConfig {
    /// Human-readable label, kept around for future tooltip / a11y use.
    #[allow(dead_code)]
    pub label: String,
    pub icon: &'static str,
    pub action: ToolbarAction,
    pub group: ToolbarGroup,
    pub style: ButtonStyle,
}
