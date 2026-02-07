use std::{collections::HashMap, env, fs, ops::Range, path::PathBuf, process::Command, sync::Arc};

use ab_glyph::{Font, FontArc, Glyph, GlyphId, PxScale, ScaleFont, point};
use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use tree_sitter::Parser;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

const FONT_PX: f32 = 18.0;
const SIDE_PADDING: f32 = 16.0;
const TOP_PADDING: f32 = 14.0;
const ATLAS_SIZE: u32 = 4096;

const COLOR_BG: wgpu::Color = wgpu::Color {
    r: 0.04,
    g: 0.045,
    b: 0.05,
    a: 1.0,
};

const COLOR_ROW_SELECTED: [f32; 4] = [0.18, 0.26, 0.38, 0.45];

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct TextVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RectVertex {
    pos: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    screen_w: f32,
    screen_h: f32,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

#[derive(Clone, Copy, Debug)]
struct GlyphUV {
    u0: f32,
    v0: f32,
    u1: f32,
    v1: f32,
    w: u32,
    h: u32,
    bearing_x: f32,
    bearing_y: f32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct GlyphKey {
    glyph_id: u16,
    px_q: u16,
}

#[derive(Clone, Copy, Debug)]
enum LineStyle {
    Normal,
    Dim,
    Header,
    Selected,
    DiffAdd,
    DiffRemove,
    DiffHunk,
}

impl LineStyle {
    fn color(self) -> [f32; 4] {
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
struct ShapedGlyph {
    glyph_id: u16,
    x: f32,
    y: f32,
    color: [f32; 4],
}

#[derive(Clone, Debug)]
struct VisualLine {
    y_top: f32,
    range: Range<usize>,
    style: LineStyle,
    glyphs: Vec<ShapedGlyph>,
    shaped: bool,
}

#[derive(Clone, Debug)]
struct Document {
    text: String,
    line_starts: Vec<usize>,
    line_styles: Vec<LineStyle>,
}

impl Document {
    fn from_lines(lines: &[(String, LineStyle)]) -> Self {
        let mut text = String::new();
        let mut line_starts = vec![0usize];
        let mut line_styles = Vec::with_capacity(lines.len());

        for (line, style) in lines {
            text.push_str(line);
            text.push('\n');
            line_starts.push(text.len());
            line_styles.push(*style);
        }

        if lines.is_empty() {
            line_styles.push(LineStyle::Dim);
        }

        Self {
            text,
            line_starts,
            line_styles,
        }
    }

    fn line_count(&self) -> usize {
        self.line_styles.len()
    }

    fn line_range_without_newline(&self, line_index: usize) -> Range<usize> {
        let start = self.line_starts[line_index];
        let next_start = self.line_starts[line_index + 1];
        let end = if next_start > start && self.text.as_bytes()[next_start - 1] == b'\n' {
            next_start - 1
        } else {
            next_start
        };
        start..end
    }

    fn line_style(&self, line_index: usize) -> LineStyle {
        self.line_styles[line_index]
    }
}

struct Atlas {
    tex: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    w: u32,
    h: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_h: u32,
}

impl Atlas {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = tex.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            tex,
            view,
            sampler,
            w,
            h,
            cursor_x: 0,
            cursor_y: 0,
            shelf_h: 0,
        }
    }

    fn alloc(&mut self, gw: u32, gh: u32) -> Option<(u32, u32)> {
        if gw > self.w || gh > self.h {
            return None;
        }

        if self.cursor_x + gw > self.w {
            self.cursor_x = 0;
            self.cursor_y += self.shelf_h.max(1);
            self.shelf_h = 0;
        }

        if self.cursor_y + gh > self.h {
            return None;
        }

        let pos = (self.cursor_x, self.cursor_y);
        self.cursor_x += gw;
        self.shelf_h = self.shelf_h.max(gh);
        Some(pos)
    }
}

#[derive(Clone, Debug)]
struct GitEntry {
    xy: String,
    path: String,
}

#[derive(Clone, Copy, Debug)]
struct GitViewMeta {
    files_start_line: usize,
    files_count: usize,
}

struct GitModel {
    repo_root: PathBuf,
    branch: String,
    entries: Vec<GitEntry>,
    selected: usize,
    diff: String,
    _ts_parser: Parser,
}

impl GitModel {
    fn open() -> anyhow::Result<Self> {
        let cwd = env::current_dir()?;
        let _repo = gix::discover(&cwd).context("not inside a git repository")?;

        let mut s = Self {
            repo_root: cwd,
            branch: String::new(),
            entries: Vec::new(),
            selected: 0,
            diff: String::new(),
            _ts_parser: Parser::new(),
        };

        s.refresh()?;
        Ok(s)
    }

    fn run_git(&self, args: &[&str]) -> anyhow::Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(args)
            .output()
            .with_context(|| format!("failed to run git command: git {}", args.join(" ")))?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            anyhow::bail!("git {} failed: {}", args.join(" "), stderr.trim());
        }

        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    fn refresh(&mut self) -> anyhow::Result<()> {
        self.branch = self
            .run_git(&["rev-parse", "--abbrev-ref", "HEAD"])?
            .trim()
            .to_string();

        let status = self.run_git(&["status", "--porcelain=v1"])?;
        self.entries = parse_porcelain_status(&status);

        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }

        self.refresh_diff()
    }

    fn refresh_diff(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            self.diff = String::from("Working tree clean. No changed files.");
            return Ok(());
        };

        let unstaged = self.run_git(&["diff", "--", &path])?;
        let staged = self.run_git(&["diff", "--cached", "--", &path])?;

        let mut out = String::new();
        if !unstaged.trim().is_empty() {
            out.push_str("# Unstaged\n");
            out.push_str(&unstaged);
            if !unstaged.ends_with('\n') {
                out.push('\n');
            }
        }
        if !staged.trim().is_empty() {
            out.push_str("# Staged\n");
            out.push_str(&staged);
            if !staged.ends_with('\n') {
                out.push('\n');
            }
        }
        if out.trim().is_empty() {
            out.push_str("No diff output for selected file.");
        }
        self.diff = out;
        Ok(())
    }

    fn move_selection(&mut self, delta: isize) -> anyhow::Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        let max = (self.entries.len() - 1) as isize;
        let next = (self.selected as isize + delta).clamp(0, max) as usize;
        if next != self.selected {
            self.selected = next;
            self.refresh_diff()?;
        }
        Ok(())
    }

    fn stage_selected(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            return Ok(());
        };
        self.run_git(&["add", "--", &path])?;
        self.refresh()
    }

    fn unstage_selected(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            return Ok(());
        };

        if self.run_git(&["restore", "--staged", "--", &path]).is_err() {
            let _ = self.run_git(&["reset", "HEAD", "--", &path])?;
        }

        self.refresh()
    }

    fn select_file_index(&mut self, idx: usize) -> anyhow::Result<()> {
        if idx < self.entries.len() && idx != self.selected {
            self.selected = idx;
            self.refresh_diff()?;
        }
        Ok(())
    }

    fn build_document(&self) -> (Document, GitViewMeta) {
        let mut lines: Vec<(String, LineStyle)> = Vec::new();

        lines.push((
            format!(
                "wgit  branch:{}  files:{}  [j/k or arrows] select  [s] stage  [u] unstage  [r] refresh  [q] quit",
                self.branch,
                self.entries.len()
            ),
            LineStyle::Header,
        ));
        lines.push((String::new(), LineStyle::Dim));
        lines.push((String::from("FILES"), LineStyle::Header));

        let files_start_line = lines.len();
        if self.entries.is_empty() {
            lines.push((String::from("  (working tree clean)"), LineStyle::Dim));
        } else {
            for (idx, e) in self.entries.iter().enumerate() {
                let marker = if idx == self.selected { ">" } else { " " };
                let style = if idx == self.selected {
                    LineStyle::Selected
                } else {
                    LineStyle::Normal
                };
                lines.push((format!("{} {} {}", marker, e.xy, e.path), style));
            }
        }
        let files_count = self.entries.len();

        lines.push((String::new(), LineStyle::Dim));
        let diff_title = self
            .entries
            .get(self.selected)
            .map(|e| format!("DIFF {}", e.path))
            .unwrap_or_else(|| String::from("DIFF"));
        lines.push((diff_title, LineStyle::Header));

        for line in self.diff.lines() {
            let normalized = normalize_for_display(line);
            lines.push((normalized, style_for_diff_line(line)));
        }

        (
            Document::from_lines(&lines),
            GitViewMeta {
                files_start_line,
                files_count,
            },
        )
    }
}

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: PhysicalSize<u32>,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,

    text_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    text_bg: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,

    atlas: Atlas,
    glyph_cache: HashMap<GlyphKey, GlyphUV>,
    font: FontArc,
    cell_width: f32,
    line_height: f32,
    ascent: f32,

    git: GitModel,
    doc: Document,
    view_meta: GitViewMeta,
    visual_lines: Vec<VisualLine>,

    scroll_y: f32,
    content_height: f32,
    mouse_pos: PhysicalPosition<f64>,

    text_vbuf: wgpu::Buffer,
    text_vcount: u32,
    row_vbuf: wgpu::Buffer,
    row_vcount: u32,

    layout_dirty: bool,
    geometry_dirty: bool,
}

impl State {
    async fn new(window: Arc<Window>, git: GitModel) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let size = window.inner_size();
        let surface = instance.create_surface(window.clone())?;
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let atlas = Atlas::new(
            &device,
            wgpu::TextureFormat::R8Unorm,
            ATLAS_SIZE,
            ATLAS_SIZE,
        );
        let glyph_cache = HashMap::new();
        let font = load_primary_font().context("failed to load font")?;
        let (doc, view_meta) = git.build_document();

        let text_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("text_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let text_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("text_bg"),
            layout: &text_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas.sampler),
                },
            ],
        });

        let uniform_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("uniform_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_init = Uniforms {
            screen_w: size.width as f32,
            screen_h: size.height as f32,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniform_buf"),
            contents: bytemuck::bytes_of(&uniform_init),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("uniform_bg"),
            layout: &uniform_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        let text_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("text_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./text.wgsl").into()),
        });
        let rect_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rect_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./rect.wgsl").into()),
        });

        let text_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("text_pl"),
            bind_group_layouts: &[&text_bgl, &uniform_bgl],
            push_constant_ranges: &[],
        });
        let rect_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rect_pl"),
            bind_group_layouts: &[&uniform_bgl],
            push_constant_ranges: &[],
        });

        let target_format = surface_format.add_srgb_suffix();

        let text_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("text_pipeline"),
            layout: Some(&text_pl),
            vertex: wgpu::VertexState {
                module: &text_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TextVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 8,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: 16,
                            shader_location: 2,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &text_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let rect_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rect_pipeline"),
            layout: Some(&rect_pl),
            vertex: wgpu::VertexState {
                module: &rect_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<RectVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    }],
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &rect_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let empty_text_vbuf = create_empty_buffer(
            &device,
            "empty_text",
            std::mem::size_of::<TextVertex>() as u64,
        );
        let empty_rect_vbuf = create_empty_buffer(
            &device,
            "empty_rect",
            std::mem::size_of::<RectVertex>() as u64,
        );

        let mut state = Self {
            window,
            device,
            queue,
            size,
            surface,
            surface_format,
            text_pipeline,
            rect_pipeline,
            text_bg,
            uniform_buf,
            uniform_bg,
            atlas,
            glyph_cache,
            font,
            cell_width: FONT_PX,
            line_height: FONT_PX * 1.3,
            ascent: FONT_PX,
            git,
            doc,
            view_meta,
            visual_lines: Vec::new(),
            scroll_y: 0.0,
            content_height: 0.0,
            mouse_pos: PhysicalPosition::new(0.0, 0.0),
            text_vbuf: empty_text_vbuf,
            text_vcount: 0,
            row_vbuf: empty_rect_vbuf,
            row_vcount: 0,
            layout_dirty: true,
            geometry_dirty: true,
        };

        state.configure_surface();
        state.compute_font_metrics();
        state.rebuild_layout();
        state.rebuild_visible_geometry()?;
        Ok(state)
    }

    fn configure_surface(&self) {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.surface_format,
            view_formats: vec![self.surface_format.add_srgb_suffix()],
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            width: self.size.width,
            height: self.size.height,
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
        };
        self.surface.configure(&self.device, &config);
    }

    fn compute_font_metrics(&mut self) {
        let scaled = self.font.as_scaled(PxScale::from(FONT_PX));
        let ascent = scaled.ascent().ceil();
        let descent = scaled.descent().floor();
        let line_gap = scaled.line_gap();
        self.ascent = ascent.max(FONT_PX * 0.7);
        self.line_height = (ascent - descent + line_gap).ceil().max(FONT_PX * 1.2);

        let adv_space = scaled.h_advance(self.font.glyph_id(' '));
        let adv_m = scaled.h_advance(self.font.glyph_id('m'));
        self.cell_width = adv_space.max(adv_m).ceil().max(1.0);
    }

    fn update_uniform_color(&self, color: [f32; 4]) {
        let u = Uniforms {
            screen_w: self.size.width as f32,
            screen_h: self.size.height as f32,
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }

    fn refresh_document_from_git(&mut self) {
        let (doc, meta) = self.git.build_document();
        self.doc = doc;
        self.view_meta = meta;
        self.layout_dirty = true;
        self.geometry_dirty = true;
    }

    fn rebuild_layout(&mut self) {
        self.visual_lines.clear();
        let mut y = TOP_PADDING;

        for line_idx in 0..self.doc.line_count() {
            self.visual_lines.push(VisualLine {
                y_top: y,
                range: self.doc.line_range_without_newline(line_idx),
                style: self.doc.line_style(line_idx),
                glyphs: Vec::new(),
                shaped: false,
            });
            y += self.line_height;
        }

        self.content_height = (y + TOP_PADDING).max(self.size.height as f32);
        self.clamp_scroll();
        self.layout_dirty = false;
        self.geometry_dirty = true;
    }

    fn ensure_visual_line_shaped(&mut self, idx: usize) {
        if idx >= self.visual_lines.len() || self.visual_lines[idx].shaped {
            return;
        }

        let y_top = self.visual_lines[idx].y_top;
        let range = self.visual_lines[idx].range.clone();
        let style = self.visual_lines[idx].style;
        let baseline = y_top + self.ascent;

        let mut x = SIDE_PADDING;
        let mut glyphs = Vec::new();
        let line_text = &self.doc.text[range];

        for ch in line_text.chars() {
            if !ch.is_control() {
                let glyph_id = self.font.glyph_id(ch).0;
                glyphs.push(ShapedGlyph {
                    glyph_id,
                    x,
                    y: baseline,
                    color: style.color(),
                });
            }
            x += self.cell_width;
        }

        self.visual_lines[idx].glyphs = glyphs;
        self.visual_lines[idx].shaped = true;
    }

    fn ensure_glyph(&mut self, glyph_id: u16) -> anyhow::Result<Option<GlyphUV>> {
        let key = GlyphKey {
            glyph_id,
            px_q: (FONT_PX * 64.0) as u16,
        };

        if let Some(uv) = self.glyph_cache.get(&key).copied() {
            return Ok(Some(uv));
        }

        let glyph = Glyph {
            id: GlyphId(glyph_id),
            scale: PxScale::from(FONT_PX),
            position: point(0.0, 0.0),
        };

        let Some(outlined) = self.font.outline_glyph(glyph) else {
            let uv = GlyphUV {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                w: 0,
                h: 0,
                bearing_x: 0.0,
                bearing_y: 0.0,
            };
            self.glyph_cache.insert(key, uv);
            return Ok(Some(uv));
        };

        let bounds = outlined.px_bounds();
        let width = (bounds.max.x - bounds.min.x).ceil().max(0.0) as u32;
        let height = (bounds.max.y - bounds.min.y).ceil().max(0.0) as u32;

        if width == 0 || height == 0 {
            let uv = GlyphUV {
                u0: 0.0,
                v0: 0.0,
                u1: 0.0,
                v1: 0.0,
                w: 0,
                h: 0,
                bearing_x: bounds.min.x,
                bearing_y: bounds.min.y,
            };
            self.glyph_cache.insert(key, uv);
            return Ok(Some(uv));
        }

        let mut alpha = vec![0u8; (width * height) as usize];
        outlined.draw(|x, y, v| {
            let i = (y as u32 * width + x as u32) as usize;
            alpha[i] = (v * 255.0 + 0.5) as u8;
        });

        let pad = 1u32;
        let gw = width + pad * 2;
        let gh = height + pad * 2;
        let (x, y) = self
            .atlas
            .alloc(gw, gh)
            .context("glyph atlas full; increase ATLAS_SIZE")?;

        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bpr = gw.next_multiple_of(align);
        let mut tmp = vec![0u8; (bpr * gh) as usize];
        for row in 0..height {
            let src = (row * width) as usize;
            let dst = ((row + pad) * bpr + pad) as usize;
            tmp[dst..dst + width as usize].copy_from_slice(&alpha[src..src + width as usize]);
        }

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas.tex,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &tmp,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(gh),
            },
            wgpu::Extent3d {
                width: gw,
                height: gh,
                depth_or_array_layers: 1,
            },
        );

        let uv = GlyphUV {
            u0: (x + pad) as f32 / self.atlas.w as f32,
            v0: (y + pad) as f32 / self.atlas.h as f32,
            u1: (x + pad + width) as f32 / self.atlas.w as f32,
            v1: (y + pad + height) as f32 / self.atlas.h as f32,
            w: width,
            h: height,
            bearing_x: bounds.min.x,
            bearing_y: bounds.min.y,
        };

        self.glyph_cache.insert(key, uv);
        Ok(Some(uv))
    }

    fn selected_file_line_index(&self) -> Option<usize> {
        if self.view_meta.files_count == 0 {
            return None;
        }
        Some(self.view_meta.files_start_line + self.git.selected)
    }

    fn rebuild_visible_geometry(&mut self) -> anyhow::Result<()> {
        let top = self.scroll_y;
        let bottom = self.scroll_y + self.size.height as f32;

        let mut text_vertices = Vec::<TextVertex>::new();
        let mut row_vertices = Vec::<RectVertex>::new();

        let visible_indices: Vec<usize> = self
            .visual_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !(line.y_top + self.line_height < top || line.y_top > bottom))
            .map(|(idx, _)| idx)
            .collect();

        let selected_line = self.selected_file_line_index();

        for idx in visible_indices {
            self.ensure_visual_line_shaped(idx);

            let (y_top, glyphs) = {
                let line = &self.visual_lines[idx];
                (line.y_top, line.glyphs.clone())
            };

            if Some(idx) == selected_line {
                let y0 = y_top - self.scroll_y;
                let y1 = y0 + self.line_height;
                push_rect(&mut row_vertices, 0.0, y0, self.size.width as f32, y1);
            }

            for g in glyphs {
                let uv = match self.ensure_glyph(g.glyph_id)? {
                    Some(uv) => uv,
                    None => continue,
                };
                if uv.w == 0 || uv.h == 0 {
                    continue;
                }

                let x0 = (g.x + uv.bearing_x).round();
                let y0 = (g.y + uv.bearing_y - self.scroll_y).round();
                let x1 = x0 + uv.w as f32;
                let y1 = y0 + uv.h as f32;

                let color = g.color;
                text_vertices.push(TextVertex {
                    pos: [x0, y0],
                    uv: [uv.u0, uv.v0],
                    color,
                });
                text_vertices.push(TextVertex {
                    pos: [x1, y0],
                    uv: [uv.u1, uv.v0],
                    color,
                });
                text_vertices.push(TextVertex {
                    pos: [x0, y1],
                    uv: [uv.u0, uv.v1],
                    color,
                });
                text_vertices.push(TextVertex {
                    pos: [x0, y1],
                    uv: [uv.u0, uv.v1],
                    color,
                });
                text_vertices.push(TextVertex {
                    pos: [x1, y0],
                    uv: [uv.u1, uv.v0],
                    color,
                });
                text_vertices.push(TextVertex {
                    pos: [x1, y1],
                    uv: [uv.u1, uv.v1],
                    color,
                });
            }
        }

        self.text_vbuf = create_vertex_buffer(&self.device, "text_vertices", &text_vertices);
        self.text_vcount = text_vertices.len() as u32;

        self.row_vbuf = create_vertex_buffer(&self.device, "row_vertices", &row_vertices);
        self.row_vcount = row_vertices.len() as u32;

        self.geometry_dirty = false;
        Ok(())
    }

    fn clamp_scroll(&mut self) {
        let max_scroll = (self.content_height - self.size.height as f32).max(0.0);
        if self.scroll_y < 0.0 {
            self.scroll_y = 0.0;
        }
        if self.scroll_y > max_scroll {
            self.scroll_y = max_scroll;
        }
    }

    fn line_index_at_doc_y(&self, y: f32) -> Option<usize> {
        self.visual_lines
            .iter()
            .enumerate()
            .find(|(_, line)| y >= line.y_top && y < line.y_top + self.line_height)
            .map(|(idx, _)| idx)
            .or_else(|| self.visual_lines.len().checked_sub(1))
    }

    fn try_select_file_from_mouse(&mut self, pos: PhysicalPosition<f64>) -> anyhow::Result<bool> {
        if self.view_meta.files_count == 0 {
            return Ok(false);
        }

        let doc_y = pos.y as f32 + self.scroll_y;
        let Some(line_idx) = self.line_index_at_doc_y(doc_y) else {
            return Ok(false);
        };

        let start = self.view_meta.files_start_line;
        let end = start + self.view_meta.files_count;
        if line_idx < start || line_idx >= end {
            return Ok(false);
        }

        let target = line_idx - start;
        if target == self.git.selected {
            return Ok(false);
        }

        self.git.select_file_index(target)?;
        self.refresh_document_from_git();
        Ok(true)
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.configure_surface();
        self.layout_dirty = true;
        self.geometry_dirty = true;
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let dy = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y * self.line_height * 3.0,
            MouseScrollDelta::PixelDelta(p) => -(p.y as f32),
        };
        self.scroll_y += dy;
        self.clamp_scroll();
        self.geometry_dirty = true;
    }

    fn handle_key(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::ArrowUp) => {
                self.git.move_selection(-1)?;
                self.refresh_document_from_git();
                Ok(true)
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.git.move_selection(1)?;
                self.refresh_document_from_git();
                Ok(true)
            }
            Key::Named(NamedKey::PageUp) => {
                self.scroll_y -= self.size.height as f32 * 0.85;
                self.clamp_scroll();
                self.geometry_dirty = true;
                Ok(true)
            }
            Key::Named(NamedKey::PageDown) => {
                self.scroll_y += self.size.height as f32 * 0.85;
                self.clamp_scroll();
                self.geometry_dirty = true;
                Ok(true)
            }
            Key::Character(ch) => {
                let c = ch.as_ref().to_ascii_lowercase();
                match c.as_str() {
                    "j" => {
                        self.git.move_selection(1)?;
                        self.refresh_document_from_git();
                        Ok(true)
                    }
                    "k" => {
                        self.git.move_selection(-1)?;
                        self.refresh_document_from_git();
                        Ok(true)
                    }
                    "r" => {
                        self.git.refresh()?;
                        self.refresh_document_from_git();
                        Ok(true)
                    }
                    "s" => {
                        self.git.stage_selected()?;
                        self.refresh_document_from_git();
                        Ok(true)
                    }
                    "u" => {
                        self.git.unstage_selected()?;
                        self.refresh_document_from_git();
                        Ok(true)
                    }
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    fn render(&mut self) -> anyhow::Result<()> {
        if self.layout_dirty {
            self.rebuild_layout();
        }
        if self.geometry_dirty {
            self.rebuild_visible_geometry()?;
        }

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                self.configure_surface();
                return Ok(());
            }
        };

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.surface_format.add_srgb_suffix()),
            ..Default::default()
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(COLOR_BG),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if self.row_vcount > 0 {
                self.update_uniform_color(COLOR_ROW_SELECTED);
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bg, &[]);
                pass.set_vertex_buffer(0, self.row_vbuf.slice(..));
                pass.draw(0..self.row_vcount, 0..1);
            }

            if self.text_vcount > 0 {
                pass.set_pipeline(&self.text_pipeline);
                pass.set_bind_group(0, &self.text_bg, &[]);
                pass.set_bind_group(1, &self.uniform_bg, &[]);
                pass.set_vertex_buffer(0, self.text_vbuf.slice(..));
                pass.draw(0..self.text_vcount, 0..1);
            }
        }

        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        frame.present();
        Ok(())
    }
}

fn create_empty_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

fn create_vertex_buffer<T: Pod>(device: &wgpu::Device, label: &str, verts: &[T]) -> wgpu::Buffer {
    if verts.is_empty() {
        return create_empty_buffer(device, label, std::mem::size_of::<T>() as u64);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(verts),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

fn push_rect(out: &mut Vec<RectVertex>, x0: f32, y0: f32, x1: f32, y1: f32) {
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    out.push(RectVertex { pos: [x0, y0] });
    out.push(RectVertex { pos: [x1, y0] });
    out.push(RectVertex { pos: [x0, y1] });
    out.push(RectVertex { pos: [x0, y1] });
    out.push(RectVertex { pos: [x1, y0] });
    out.push(RectVertex { pos: [x1, y1] });
}

fn load_primary_font() -> anyhow::Result<FontArc> {
    let candidates = [
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/Menlo.ttc",
        "/System/Library/Fonts/Supplemental/Menlo.ttc",
        "/System/Library/Fonts/Monaco.ttf",
    ];

    for path in candidates {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(font) = FontArc::try_from_vec(bytes) {
                return Ok(font);
            }
        }
    }

    FontArc::try_from_slice(include_bytes!("../data/fonts/Terminus.ttf"))
        .context("failed to load built-in Terminus.ttf")
}

fn parse_porcelain_status(status: &str) -> Vec<GitEntry> {
    let mut entries = Vec::new();

    for line in status.lines() {
        if line.len() < 4 {
            continue;
        }
        let x = line.as_bytes()[0] as char;
        let y = line.as_bytes()[1] as char;
        let mut path = line[3..].trim().to_string();
        if let Some(pos) = path.rfind(" -> ") {
            path = path[(pos + 4)..].to_string();
        }
        entries.push(GitEntry {
            xy: format!("{}{}", x, y),
            path,
        });
    }

    entries
}

fn style_for_diff_line(line: &str) -> LineStyle {
    if line.starts_with("@@") {
        LineStyle::DiffHunk
    } else if line.starts_with('+') && !line.starts_with("+++") {
        LineStyle::DiffAdd
    } else if line.starts_with('-') && !line.starts_with("---") {
        LineStyle::DiffRemove
    } else if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
        || line.starts_with("# ")
    {
        LineStyle::Dim
    } else {
        LineStyle::Normal
    }
}

fn normalize_for_display(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut col = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                out.push('\n');
                col = 0;
            }
            '\n' => {
                out.push('\n');
                col = 0;
            }
            '\t' => {
                let spaces = 4 - (col % 4);
                for _ in 0..spaces {
                    out.push(' ');
                }
                col += spaces;
            }
            _ => {
                out.push(ch);
                col += 1;
            }
        }
    }

    out
}

struct App {
    state: Option<State>,
    git: Option<GitModel>,
}

impl App {
    fn new(git: GitModel) -> Self {
        Self {
            state: None,
            git: Some(git),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("wgit");
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let git = self.git.take().expect("git model available");
        let state = pollster::block_on(State::new(window.clone(), git)).expect("init state");
        self.state = Some(state);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = self.state.as_mut() else {
            return;
        };

        let mut needs_redraw = false;

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(sz) => {
                st.on_resize(sz);
                needs_redraw = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                st.mouse_pos = position;
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state == ElementState::Pressed {
                    match st.try_select_file_from_mouse(st.mouse_pos) {
                        Ok(changed) => needs_redraw |= changed,
                        Err(err) => eprintln!("select error: {err}"),
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                st.on_wheel(delta);
                needs_redraw = true;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Key::Character(ch) = &event.logical_key {
                        if ch.eq_ignore_ascii_case("q") {
                            event_loop.exit();
                            return;
                        }
                    }

                    match st.handle_key(&event.logical_key) {
                        Ok(changed) => needs_redraw |= changed,
                        Err(err) => eprintln!("key handling error: {err}"),
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = st.render() {
                    eprintln!("render error: {err}");
                    event_loop.exit();
                    return;
                }
            }
            _ => {}
        }

        if needs_redraw {
            st.window.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();

    let git = match GitModel::open() {
        Ok(git) => git,
        Err(err) => {
            eprintln!("Failed to open git repository: {err}");
            std::process::exit(1);
        }
    };

    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(git);
    event_loop.run_app(&mut app).expect("run app");
}
