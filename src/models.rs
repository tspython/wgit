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
            Self::Normal => theme::LINE_NORMAL,
            Self::Dim => theme::LINE_DIM,
            Self::Header => theme::LINE_HEADER,
            Self::Selected => theme::LINE_SELECTED,
            Self::DiffAdd => theme::LINE_DIFF_ADD,
            Self::DiffRemove => theme::LINE_DIFF_REMOVE,
            Self::DiffHunk => theme::LINE_DIFF_HUNK,
            Self::DiffMeta => theme::LINE_DIM,
            Self::DiffFileHeader => theme::TEXT_ACCENT,
            Self::SectionStaged => theme::ACCENT_GREEN,
            Self::SectionUnstaged => theme::ACCENT_YELLOW,
            Self::SectionUntracked => theme::ACCENT_GRAY,
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
                theme::DIFF_ADD_BG_TOP,
                theme::DIFF_ADD_BG_BOTTOM,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffRemove => (
                theme::DIFF_REMOVE_BG_TOP,
                theme::DIFF_REMOVE_BG_BOTTOM,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffHunk => (
                theme::DIFF_HUNK_BG_TOP,
                theme::DIFF_HUNK_BG_BOTTOM,
                theme::DIFF_HUNK_BORDER,
            ),
            Self::DiffMeta => (
                theme::DIFF_META_BG_TOP,
                theme::DIFF_META_BG_BOTTOM,
                [0.0, 0.0, 0.0, 0.0],
            ),
            Self::DiffFileHeader => (
                theme::DIFF_FILE_HEADER_BG_TOP,
                theme::DIFF_FILE_HEADER_BG_BOTTOM,
                theme::DIFF_FILE_HEADER_BORDER,
            ),
            Self::SectionStaged => (
                theme::SECTION_STAGED_BG_TOP,
                theme::SECTION_STAGED_BG_BOTTOM,
                theme::SECTION_STAGED_BORDER,
            ),
            Self::SectionUnstaged => (
                theme::SECTION_UNSTAGED_BG_TOP,
                theme::SECTION_UNSTAGED_BG_BOTTOM,
                theme::SECTION_UNSTAGED_BORDER,
            ),
            Self::SectionUntracked => (
                theme::SECTION_UNTRACKED_BG_TOP,
                theme::SECTION_UNTRACKED_BG_BOTTOM,
                theme::SECTION_UNTRACKED_BORDER,
            ),
            _ => ([0.0; 4], [0.0; 4], [0.0; 4]),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ToolbarAction {
    RepoSwitch,
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

    #[allow(dead_code)] // builder method, part of DocLine's public API
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
    pub label: String,
    pub action: ToolbarAction,
    pub group: ToolbarGroup,
    pub style: ButtonStyle,
}
