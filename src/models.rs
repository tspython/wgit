#[derive(Clone, Copy, Debug)]
pub enum LineStyle {
    Normal,
    Dim,
    Header,
    Selected,
    DiffAdd,
    DiffRemove,
    DiffHunk,
}

impl LineStyle {
    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Normal => [0.91, 0.93, 0.96, 1.0],
            Self::Dim => [0.62, 0.67, 0.74, 1.0],
            Self::Header => [0.80, 0.87, 1.0, 1.0],
            Self::Selected => [1.0, 1.0, 1.0, 1.0],
            Self::DiffAdd => [0.63, 0.93, 0.68, 1.0],
            Self::DiffRemove => [0.98, 0.63, 0.63, 1.0],
            Self::DiffHunk => [0.62, 0.78, 1.0, 1.0],
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

#[derive(Clone, Debug)]
pub struct DocLine {
    pub text: String,
    pub style: LineStyle,
    pub spans: Vec<ColorSpan>,
}

#[derive(Clone, Debug)]
pub struct Document {
    lines: Vec<DocLine>,
}

impl Document {
    pub fn from_lines(mut lines: Vec<DocLine>) -> Self {
        if lines.is_empty() {
            lines.push(DocLine {
                text: String::new(),
                style: LineStyle::Dim,
                spans: Vec::new(),
            });
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
    pub style: ButtonStyle,
}
