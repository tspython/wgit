use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use ab_glyph::{Font, FontArc, Glyph, GlyphId, PxScale, ScaleFont, point};
use anyhow::Context;
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Modifiers, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{CursorIcon, Window, WindowId},
};

use crate::git_model::{BranchTrackingStatus, GitModel, GroupedGitViewMeta};
use crate::models::{
    ButtonConfig, ButtonStyle, Document, FocusPane, LineStyle, ShapedGlyph, ToolbarAction,
    ToolbarButton, ToolbarGroup, VisualLine, WindowControlAction, WindowControlButton,
};
use crate::render::{
    Atlas, GlyphCache, GlyphKey, GlyphUV, QuadVertex, StyledRectInstance, TextVertex, Uniforms,
    create_empty_buffer, create_vertex_buffer, push_styled_rect, push_styled_rect_glow,
};
use crate::repo_store;
use crate::theme::{
    self, ATLAS_SIZE, FONT_PX, SIDE_PADDING, STATUS_BAR_GAP, STATUS_BAR_HEIGHT,
    STATUS_BAR_SIDE_PADDING, TOP_PADDING,
};

const TOOLTIP_DELAY: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
enum StatusKind {
    Neutral,
    Success,
    Error,
    Prompt,
}

#[derive(Clone, Debug)]
struct AppStatus {
    kind: StatusKind,
    message: String,
}

impl AppStatus {
    fn new(kind: StatusKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

enum PaneRef {
    Files,
    Diff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputMode {
    Normal,
    CommitSummary,
    CommitBody,
    RepoPicker,
    DiscardConfirm,
    BranchSwitcher,
    Settings,
}

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: PhysicalSize<u32>,
    ui_scale: f32,
    surface: wgpu::Surface<'static>,
    surface_format: wgpu::TextureFormat,

    text_pipeline: wgpu::RenderPipeline,
    rect_pipeline: wgpu::RenderPipeline,
    text_bg: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    uniform_bg: wgpu::BindGroup,

    atlas: Atlas,
    glyph_cache: GlyphCache,
    icons: std::collections::HashMap<&'static str, GlyphUV>,
    font: FontArc,
    cell_width: f32,
    line_height: f32,
    ascent: f32,

    git: GitModel,

    // ── File pane (left) ──────────────────────────────────
    file_doc: Document,
    file_line_to_index: Vec<Option<usize>>,
    file_index_to_line: Vec<usize>,
    file_visual_lines: Vec<VisualLine>,
    file_scroll_y: f32,
    file_content_height: f32,

    // ── Diff pane (right) ─────────────────────────────────
    diff_doc: Document,
    diff_visual_lines: Vec<VisualLine>,
    diff_scroll_y: f32,
    diff_content_height: f32,

    // ── Focus ─────────────────────────────────────────────
    focus_pane: FocusPane,

    status: AppStatus,
    input_mode: InputMode,
    commit_summary: String,
    commit_body: String,
    repo_tracking: BranchTrackingStatus,
    recent_repos: Vec<PathBuf>,
    repo_picker_index: usize,
    repo_picker_scroll: usize,
    pending_discard_path: Option<PathBuf>,

    // ── Branch switcher ───────────────────────────────────────
    branch_list: Vec<String>,
    branch_current: String,
    branch_picker_index: usize,
    branch_picker_scroll: usize,

    mouse_pos: PhysicalPosition<f64>,
    window_controls: Vec<WindowControlButton>,
    toolbar_buttons: Vec<ToolbarButton>,

    hover_toolbar_action: Option<ToolbarAction>,
    hover_started_at: Option<Instant>,
    tooltip_drawn: bool,

    // ── Settings modal ───────────────────────────────────
    settings_index: usize,
    theme_index: usize,

    // ── Panel split ratio ────────────────────────────────
    file_pane_ratio: f32,
    divider_dragging: bool,

    // ── Zoom ─────────────────────────────────────────────
    zoom_level: f32,

    text_vbuf: wgpu::Buffer,
    text_vcount: u32,
    rect_unit_vbuf: wgpu::Buffer,
    rect_instance_vbuf: wgpu::Buffer,
    rect_instance_count: u32,

    layout_dirty: bool,
    geometry_dirty: bool,
}

impl State {
    async fn new(window: Arc<Window>, mut git: GitModel) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let size = window.inner_size();
        let ui_scale = compute_ui_scale(window.scale_factor(), size);
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
        let glyph_cache = GlyphCache::new();
        let font = load_primary_font().context("failed to load font")?;
        let (file_doc, view_meta, diff_doc) = git.build_split_documents()?;
        let (file_line_to_index, file_index_to_line) =
            build_grouped_file_maps(&file_doc, &view_meta);

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
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<QuadVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        }],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<StyledRectInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 16,
                                shader_location: 2,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 32,
                                shader_location: 3,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 48,
                                shader_location: 4,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 64,
                                shader_location: 5,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 80,
                                shader_location: 6,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                            wgpu::VertexAttribute {
                                offset: 96,
                                shader_location: 7,
                                format: wgpu::VertexFormat::Float32x4,
                            },
                        ],
                    },
                ],
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
        let empty_rect_inst_vbuf = create_empty_buffer(
            &device,
            "empty_rect_instances",
            std::mem::size_of::<StyledRectInstance>() as u64,
        );

        let rect_unit_verts = [
            QuadVertex { unit: [0.0, 0.0] },
            QuadVertex { unit: [1.0, 0.0] },
            QuadVertex { unit: [0.0, 1.0] },
            QuadVertex { unit: [0.0, 1.0] },
            QuadVertex { unit: [1.0, 0.0] },
            QuadVertex { unit: [1.0, 1.0] },
        ];
        let rect_unit_vbuf = create_vertex_buffer(&device, "rect_unit", &rect_unit_verts);

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
            icons: std::collections::HashMap::new(),
            font,
            cell_width: FONT_PX * ui_scale,
            line_height: FONT_PX * ui_scale * 1.3,
            ascent: FONT_PX * ui_scale,
            ui_scale,
            git,
            file_doc,
            file_line_to_index,
            file_index_to_line,
            file_visual_lines: Vec::new(),
            file_scroll_y: 0.0,
            file_content_height: 0.0,
            diff_doc,
            diff_visual_lines: Vec::new(),
            diff_scroll_y: 0.0,
            diff_content_height: 0.0,
            focus_pane: FocusPane::Files,
            status: AppStatus::new(StatusKind::Neutral, "Ready"),
            input_mode: InputMode::Normal,
            commit_summary: String::new(),
            commit_body: String::new(),
            repo_tracking: BranchTrackingStatus::default(),
            recent_repos: Vec::new(),
            repo_picker_index: 0,
            repo_picker_scroll: 0,
            pending_discard_path: None,
            branch_list: Vec::new(),
            branch_current: String::new(),
            branch_picker_index: 0,
            branch_picker_scroll: 0,
            mouse_pos: PhysicalPosition::new(0.0, 0.0),
            window_controls: Vec::new(),
            toolbar_buttons: Vec::new(),
            hover_toolbar_action: None,
            hover_started_at: None,
            tooltip_drawn: false,
            settings_index: 0,
            theme_index: theme::bundled_names()
                .iter()
                .position(|n| n.eq_ignore_ascii_case(theme::palette().name))
                .unwrap_or(0),
            file_pane_ratio: 0.30,
            divider_dragging: false,
            zoom_level: 1.0,
            text_vbuf: empty_text_vbuf,
            text_vcount: 0,
            rect_unit_vbuf,
            rect_instance_vbuf: empty_rect_inst_vbuf,
            rect_instance_count: 0,
            layout_dirty: true,
            geometry_dirty: true,
        };

        state.configure_surface();
        state.update_uniform_screen();
        state.compute_font_metrics();
        state.load_toolbar_icons()?;
        state.refresh_recent_repos();
        state.refresh_repo_tracking()?;
        state.rebuild_layout();
        state.set_selection_status();
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

    fn ui(&self, value: f32) -> f32 {
        value * self.ui_scale
    }

    fn font_px(&self) -> f32 {
        FONT_PX * self.ui_scale
    }

    fn side_padding(&self) -> f32 {
        SIDE_PADDING * self.ui_scale
    }

    fn top_padding(&self) -> f32 {
        TOP_PADDING * self.ui_scale
    }

    fn status_bar_height(&self) -> f32 {
        STATUS_BAR_HEIGHT * self.ui_scale
    }

    fn status_bar_gap(&self) -> f32 {
        STATUS_BAR_GAP * self.ui_scale
    }

    fn status_bar_side_padding(&self) -> f32 {
        STATUS_BAR_SIDE_PADDING * self.ui_scale
    }

    fn refresh_ui_scale(&mut self) {
        self.ui_scale = compute_ui_scale(self.window.scale_factor(), self.size) * self.zoom_level;
        self.compute_font_metrics();
        self.layout_dirty = true;
        self.geometry_dirty = true;
    }

    fn compute_font_metrics(&mut self) {
        let font_px = self.font_px();
        let scaled = self.font.as_scaled(PxScale::from(font_px));
        let ascent = scaled.ascent().ceil();
        let descent = scaled.descent().floor();
        let line_gap = scaled.line_gap();
        self.ascent = ascent.max(font_px * 0.7);
        self.line_height = (ascent - descent + line_gap).ceil().max(font_px * 1.2);

        let adv_space = scaled.h_advance(self.font.glyph_id(' '));
        let adv_m = scaled.h_advance(self.font.glyph_id('m'));
        self.cell_width = adv_space.max(adv_m).ceil().max(1.0);
    }

    fn update_uniform_screen(&self) {
        let u = Uniforms {
            screen_w: self.size.width as f32,
            screen_h: self.size.height as f32,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }

    fn set_status(&mut self, kind: StatusKind, message: impl Into<String>) {
        let message = message.into();
        self.status = AppStatus::new(kind, compact_status_message(&message, 180));
        self.geometry_dirty = true;
    }

    fn status_fill(&self) -> ([f32; 4], [f32; 4], [f32; 4], [f32; 4]) {
        match self.status.kind {
            StatusKind::Neutral => theme::palette().status_neutral,
            StatusKind::Success => theme::palette().status_success,
            StatusKind::Error => theme::palette().status_error,
            StatusKind::Prompt => theme::palette().status_prompt,
        }
    }

    fn selection_status_message(&self) -> String {
        let count = self.git.entries_len();
        let pane_label = match self.focus_pane {
            FocusPane::Files => "FILES",
            FocusPane::Diff => "DIFF",
        };
        if count == 0 {
            format!(
                "[{pane_label}]  Working tree clean  \u{2502}  {}  \u{2502}  Tab switch pane  c commit  o repos  q quit",
                self.git.branch()
            )
        } else {
            format!(
                "[{pane_label}]  {}/{}  \u{2502}  s stage  u unstage  x discard  c commit  Tab switch pane  j/k scroll",
                self.git.selected_index() + 1,
                count,
            )
        }
    }

    fn set_selection_status(&mut self) {
        self.set_status(StatusKind::Neutral, self.selection_status_message());
    }

    fn refresh_recent_repos(&mut self) {
        let mut repos = repo_store::recent_repos().unwrap_or_default();
        let current = self.git.repo_root().to_path_buf();
        repos.retain(|repo| repo != &current);
        repos.insert(0, current);
        self.recent_repos = repos;
        self.repo_picker_index = self
            .repo_picker_index
            .min(self.recent_repos.len().saturating_sub(1));
        self.repo_picker_scroll = self.repo_picker_scroll.min(self.repo_picker_index);
    }

    fn refresh_repo_tracking(&mut self) -> anyhow::Result<()> {
        self.repo_tracking = self.git.tracking().clone();
        self.geometry_dirty = true;
        Ok(())
    }

    fn prompt_commit_message(&mut self) {
        self.input_mode = InputMode::CommitSummary;
        self.commit_summary.clear();
        self.commit_body.clear();
        self.set_status(
            StatusKind::Prompt,
            "Commit summary: type a subject, Enter for body, Esc to cancel",
        );
    }

    fn prompt_repo_picker(&mut self) {
        self.refresh_recent_repos();
        self.input_mode = InputMode::RepoPicker;
        self.set_repo_picker_index(0);
        self.update_repo_picker_prompt();
    }

    fn prompt_discard_confirm(&mut self) {
        self.input_mode = InputMode::DiscardConfirm;
        self.pending_discard_path = self.selected_file_path();
        let message = match self.pending_discard_path.as_ref() {
            Some(path) => format!(
                "Discard selected file? {}  [y/Enter] confirm  [Esc] cancel",
                path.display()
            ),
            None => String::from("Discard selected file? [y/Enter] confirm  [Esc] cancel"),
        };
        self.set_status(StatusKind::Prompt, message);
    }

    fn prompt_settings(&mut self) {
        self.input_mode = InputMode::Settings;
        self.settings_index = 0;
        self.set_status(
            StatusKind::Prompt,
            "Settings: Up/Down navigate  Left/Right change  Esc close",
        );
    }

    fn cancel_input_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.commit_summary.clear();
        self.commit_body.clear();
        self.pending_discard_path = None;
        self.branch_list.clear();
        self.set_selection_status();
    }

    fn update_commit_prompt(&mut self) {
        let message = match self.input_mode {
            InputMode::CommitSummary => {
                if self.commit_summary.is_empty() {
                    String::from("Commit summary: type a subject, Enter for body, Esc to cancel")
                } else {
                    format!(
                        "Commit summary: {}  [Enter] next  [Esc] cancel",
                        self.commit_summary
                    )
                }
            }
            InputMode::CommitBody => {
                if self.commit_body.is_empty() {
                    String::from(
                        "Commit body: optional details, Enter to submit, Tab back, Esc cancel",
                    )
                } else {
                    format!(
                        "Commit body: {}  [Enter] submit  [Tab] back  [Esc] cancel",
                        self.commit_body
                    )
                }
            }
            _ => String::from("Commit: type a subject, Enter for body, Esc to cancel"),
        };
        self.set_status(StatusKind::Prompt, message);
    }

    fn update_repo_picker_prompt(&mut self) {
        let message = if let Some(path) = self.selected_repo_path() {
            format!(
                "Repo switcher: {}  [Enter] open  [Esc] cancel",
                compact_status_message(&path.display().to_string(), 64)
            )
        } else {
            String::from("Repo switcher: use arrows, Enter to open, Esc to cancel")
        };
        self.set_status(StatusKind::Prompt, message);
    }

    fn submit_commit_message(&mut self) -> anyhow::Result<()> {
        let summary = self.commit_summary.trim().to_string();
        let body = self.commit_body.trim().to_string();
        if summary.is_empty() {
            anyhow::bail!("commit summary cannot be empty");
        }
        let summary_for_status = summary.clone();

        let message = if body.is_empty() {
            summary
        } else {
            format!("{summary}\n\n{body}")
        };

        self.git.commit(&message)?;
        self.refresh_document_from_git()?;
        self.input_mode = InputMode::Normal;
        self.commit_summary.clear();
        self.commit_body.clear();
        self.set_status(
            StatusKind::Success,
            format!("Committed changes: {summary_for_status}"),
        );
        Ok(())
    }

    fn selected_repo_path(&self) -> Option<PathBuf> {
        self.recent_repos.get(self.repo_picker_index).cloned()
    }

    fn set_repo_picker_index(&mut self, index: usize) {
        if self.recent_repos.is_empty() {
            self.repo_picker_index = 0;
            self.repo_picker_scroll = 0;
            return;
        }

        let last = self.recent_repos.len() - 1;
        self.repo_picker_index = index.min(last);
        let visible = 6usize;
        if self.repo_picker_index < self.repo_picker_scroll {
            self.repo_picker_scroll = self.repo_picker_index;
        } else if self.repo_picker_index >= self.repo_picker_scroll + visible {
            self.repo_picker_scroll = self.repo_picker_index + 1 - visible;
        }
    }

    fn open_selected_repo(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.selected_repo_path() else {
            anyhow::bail!("no repository selected");
        };

        if path == self.git.repo_root() {
            self.cancel_input_mode();
            return Ok(());
        }

        let git = GitModel::open_at(&path)
            .with_context(|| format!("failed to open repository at {}", path.display()))?;
        self.git = git;
        let _ = repo_store::remember_repo(self.git.repo_root());
        self.refresh_recent_repos();
        self.refresh_document_from_git()?;
        self.input_mode = InputMode::Normal;
        self.set_status(
            StatusKind::Success,
            format!("Opened repository {}", self.git.repo_root().display()),
        );
        Ok(())
    }

    fn submit_discard_confirm(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.pending_discard_path.clone() else {
            anyhow::bail!("nothing to discard");
        };
        self.git.discard_selected()?;
        self.refresh_document_from_git()?;
        self.pending_discard_path = None;
        self.input_mode = InputMode::Normal;
        self.set_status(StatusKind::Success, format!("Discarded {}", path.display()));
        Ok(())
    }

    fn current_commit_target(&mut self) -> &mut String {
        match self.input_mode {
            InputMode::CommitSummary => &mut self.commit_summary,
            InputMode::CommitBody => &mut self.commit_body,
            _ => &mut self.commit_summary,
        }
    }

    fn selected_file_path(&self) -> Option<PathBuf> {
        let line_idx = self.file_index_to_line.get(self.git.selected_index())?;
        let text = self.file_doc.line_text(*line_idx);
        // Text format: "  M  filename.rs" — path starts after "  X  "
        text.get(5..).map(|path| PathBuf::from(path.trim()))
    }

    fn handle_commit_input(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_input_mode();
                Ok(true)
            }
            Key::Named(NamedKey::Enter) => {
                if matches!(self.input_mode, InputMode::CommitSummary) {
                    self.input_mode = InputMode::CommitBody;
                    self.update_commit_prompt();
                } else {
                    self.submit_commit_message()?;
                }
                Ok(true)
            }
            Key::Named(NamedKey::Tab) => {
                self.input_mode = if matches!(self.input_mode, InputMode::CommitSummary) {
                    InputMode::CommitBody
                } else {
                    InputMode::CommitSummary
                };
                self.update_commit_prompt();
                Ok(true)
            }
            Key::Named(NamedKey::Backspace) => {
                self.current_commit_target().pop();
                self.update_commit_prompt();
                Ok(true)
            }
            Key::Character(ch) => {
                let mut changed = false;
                for c in ch.chars() {
                    if !c.is_control() {
                        self.current_commit_target().push(c);
                        changed = true;
                    }
                }
                if changed {
                    self.update_commit_prompt();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    fn handle_repo_picker_input(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_input_mode();
                Ok(true)
            }
            Key::Named(NamedKey::Enter) => {
                self.open_selected_repo()?;
                Ok(true)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if self.repo_picker_index > 0 {
                    self.set_repo_picker_index(self.repo_picker_index - 1);
                    self.update_repo_picker_prompt();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                if self.repo_picker_index + 1 < self.recent_repos.len() {
                    self.set_repo_picker_index(self.repo_picker_index + 1);
                    self.update_repo_picker_prompt();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Key::Named(NamedKey::Home) => {
                self.set_repo_picker_index(0);
                self.update_repo_picker_prompt();
                Ok(true)
            }
            Key::Named(NamedKey::End) => {
                if !self.recent_repos.is_empty() {
                    self.set_repo_picker_index(self.recent_repos.len() - 1);
                    self.update_repo_picker_prompt();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }


    fn handle_discard_confirm_input(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_input_mode();
                Ok(true)
            }
            Key::Named(NamedKey::Enter) => {
                self.submit_discard_confirm()?;
                Ok(true)
            }
            Key::Character(ch) if ch.eq_ignore_ascii_case("y") => {
                self.submit_discard_confirm()?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    // ── Folder browser (native OS dialog) ─────────────────────────

    fn open_folder_dialog(&mut self) -> anyhow::Result<()> {
        let start_dir = self.git.repo_root().parent()
            .unwrap_or_else(|| std::path::Path::new("/"))
            .to_path_buf();

        let picked = rfd::FileDialog::new()
            .set_title("Open Git Repository")
            .set_directory(&start_dir)
            .pick_folder();

        let Some(path) = picked else {
            return Ok(()); // user cancelled
        };

        let git = GitModel::open_at(&path)
            .with_context(|| format!("{} is not a git repository", path.display()))?;
        self.git = git;
        let _ = repo_store::remember_repo(self.git.repo_root());
        self.refresh_recent_repos();
        self.refresh_document_from_git()?;
        self.set_status(
            StatusKind::Success,
            format!("Opened repository {}", self.git.repo_root().display()),
        );
        Ok(())
    }

    // ── Branch switcher ─────────────────────────────────────────────

    fn prompt_branch_switcher(&mut self) {
        match self.git.list_branches() {
            Ok(branches) => {
                self.branch_current = self.git.branch().to_string();
                self.branch_picker_index = branches
                    .iter()
                    .position(|b| b == &self.branch_current)
                    .unwrap_or(0);
                self.branch_list = branches;
                self.branch_picker_scroll = 0;
                self.set_branch_picker_index(self.branch_picker_index);
                self.input_mode = InputMode::BranchSwitcher;
                self.update_branch_picker_prompt();
            }
            Err(err) => {
                self.set_status(StatusKind::Error, format!("Failed to list branches: {err}"));
            }
        }
    }

    fn update_branch_picker_prompt(&mut self) {
        let message = if let Some(name) = self.branch_list.get(self.branch_picker_index) {
            format!(
                "Branch: {}  [Enter] checkout  [Esc] cancel",
                compact_status_message(name, 48)
            )
        } else {
            String::from("Branch switcher: use arrows, Enter to checkout, Esc to cancel")
        };
        self.set_status(StatusKind::Prompt, message);
    }

    fn set_branch_picker_index(&mut self, index: usize) {
        if self.branch_list.is_empty() {
            self.branch_picker_index = 0;
            self.branch_picker_scroll = 0;
            return;
        }
        let last = self.branch_list.len() - 1;
        self.branch_picker_index = index.min(last);
        let visible = 6usize;
        if self.branch_picker_index < self.branch_picker_scroll {
            self.branch_picker_scroll = self.branch_picker_index;
        } else if self.branch_picker_index >= self.branch_picker_scroll + visible {
            self.branch_picker_scroll = self.branch_picker_index + 1 - visible;
        }
    }

    fn checkout_selected_branch(&mut self) -> anyhow::Result<()> {
        let Some(name) = self.branch_list.get(self.branch_picker_index).cloned() else {
            anyhow::bail!("no branch selected");
        };
        if name == self.branch_current {
            self.cancel_input_mode();
            return Ok(());
        }
        self.git.checkout_branch(&name)?;
        self.refresh_document_from_git()?;
        self.input_mode = InputMode::Normal;
        self.branch_list.clear();
        self.set_status(StatusKind::Success, format!("Switched to branch {name}"));
        Ok(())
    }

    fn handle_branch_switcher_input(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_input_mode();
                Ok(true)
            }
            Key::Named(NamedKey::Enter) => {
                self.checkout_selected_branch()?;
                Ok(true)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if self.branch_picker_index > 0 {
                    self.set_branch_picker_index(self.branch_picker_index - 1);
                    self.update_branch_picker_prompt();
                }
                Ok(true)
            }
            Key::Named(NamedKey::ArrowDown) => {
                if self.branch_picker_index + 1 < self.branch_list.len() {
                    self.set_branch_picker_index(self.branch_picker_index + 1);
                    self.update_branch_picker_prompt();
                }
                Ok(true)
            }
            Key::Named(NamedKey::Home) => {
                self.set_branch_picker_index(0);
                self.update_branch_picker_prompt();
                Ok(true)
            }
            Key::Named(NamedKey::End) => {
                if !self.branch_list.is_empty() {
                    self.set_branch_picker_index(self.branch_list.len() - 1);
                    self.update_branch_picker_prompt();
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Total number of rows in the settings modal.
    const SETTINGS_COUNT: usize = 4;

    fn handle_settings_input(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Escape) => {
                self.cancel_input_mode();
                Ok(true)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if self.settings_index > 0 {
                    self.settings_index -= 1;
                    self.geometry_dirty = true;
                }
                Ok(true)
            }
            Key::Named(NamedKey::ArrowDown) => {
                if self.settings_index + 1 < Self::SETTINGS_COUNT {
                    self.settings_index += 1;
                    self.geometry_dirty = true;
                }
                Ok(true)
            }
            Key::Named(NamedKey::ArrowRight | NamedKey::Enter) => {
                self.settings_adjust(1)?;
                Ok(true)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.settings_adjust(-1)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Adjust the setting at `self.settings_index` by the given direction.
    fn settings_adjust(&mut self, direction: i32) -> anyhow::Result<()> {
        match self.settings_index {
            // 0 = Diff backend
            0 => {
                match self.git.toggle_diff_backend() {
                    Ok(_) => {
                        self.refresh_document_from_git()?;
                    }
                    Err(e) => {
                        self.set_status(StatusKind::Error, format!("{e}"));
                    }
                }
            }
            // 1 = Pane ratio
            1 => {
                let delta = direction as f32 * 0.05;
                self.adjust_pane_ratio(delta);
            }
            // 2 = Zoom level
            2 => {
                let delta = direction as f32 * 0.10;
                self.apply_zoom(delta);
            }
            // 3 = Theme
            3 => {
                self.cycle_theme(direction);
            }
            _ => {}
        }
        self.geometry_dirty = true;
        Ok(())
    }

    /// Step through the bundled themes by `direction` (-1 / +1) and
    /// install the chosen palette atomically. The next `geometry_dirty`
    /// frame redraws every chrome surface in the new colors.
    fn cycle_theme(&mut self, direction: i32) {
        let names = theme::bundled_names();
        if names.is_empty() {
            return;
        }
        let n = names.len() as i32;
        let cur = self.theme_index as i32;
        let next = (cur + direction).rem_euclid(n) as usize;
        if let Some(p) = theme::bundled(names[next]) {
            theme::set_palette(p);
            self.theme_index = next;
        }
    }

    /// Build the label and current value for each settings row.
    fn settings_row_label(&self, index: usize) -> (&'static str, String) {
        match index {
            0 => (
                "Diff backend",
                self.git.diff_backend().label().to_string(),
            ),
            1 => (
                "Pane split",
                format!("{:.0}% / {:.0}%",
                    self.file_pane_ratio * 100.0,
                    (1.0 - self.file_pane_ratio) * 100.0),
            ),
            2 => (
                "Zoom level",
                format!("{:.0}%", self.zoom_level * 100.0),
            ),
            3 => ("Theme", theme::palette().name.to_string()),
            _ => ("", String::new()),
        }
    }

    fn execute_action<F>(&mut self, label: &str, action: F) -> bool
    where
        F: FnOnce(&mut Self) -> anyhow::Result<()>,
    {
        match action(self) {
            Ok(()) => {
                self.set_status(StatusKind::Success, label);
                true
            }
            Err(err) => {
                self.set_status(StatusKind::Error, format!("{label}: {err}"));
                false
            }
        }
    }

    fn handle_toolbar_action(&mut self, action: ToolbarAction) -> anyhow::Result<bool> {
        match action {
            ToolbarAction::Quit => Ok(false),
            ToolbarAction::RepoSwitch => {
                self.prompt_repo_picker();
                Ok(true)
            }
            ToolbarAction::Browse => {
                self.open_folder_dialog()?;
                Ok(true)
            }
            ToolbarAction::BranchSwitch => {
                self.prompt_branch_switcher();
                Ok(true)
            }
            ToolbarAction::Commit => {
                self.prompt_commit_message();
                Ok(true)
            }
            ToolbarAction::Refresh => Ok(self.execute_action("Repository refreshed", |state| {
                state.git.refresh()?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::Stage => Ok(self.execute_action("Selected file staged", |state| {
                state.git.stage_selected()?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::StageAll => Ok(self.execute_action("All changes staged", |state| {
                state.git.stage_all()?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::Unstage => Ok(self.execute_action("Selected file unstaged", |state| {
                state.git.unstage_selected()?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::UnstageAll => Ok(self.execute_action("All changes unstaged", |state| {
                state.git.unstage_all()?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::Discard => {
                self.prompt_discard_confirm();
                Ok(true)
            }
            ToolbarAction::Settings => {
                self.prompt_settings();
                self.geometry_dirty = true;
                Ok(true)
            }
            ToolbarAction::Fetch => Ok(self.execute_action("Repository fetched", |state| {
                state.git.fetch(None)?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::Pull => Ok(self.execute_action("Repository pulled", |state| {
                state.git.pull(None, None, false)?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
            ToolbarAction::Push => Ok(self.execute_action("Repository pushed", |state| {
                state.git.push(None, None, false)?;
                state.refresh_document_from_git()?;
                Ok(())
            })),
        }
    }

    fn move_selection_and_refresh(&mut self, delta: isize) -> anyhow::Result<bool> {
        let before = self.git.selected_index();
        self.git.move_selection(delta)?;
        if before == self.git.selected_index() {
            return Ok(false);
        }

        self.refresh_document_from_git()?;
        self.set_selection_status();
        Ok(true)
    }

    fn status_bar_rect(&self) -> [f32; 4] {
        self.status_bar_rect_raw()
    }

    fn build_status_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let bar = self.status_bar_rect();
        let (fill_top, fill_bottom, stroke, text) = self.status_fill();

        push_styled_rect(
            rect_instances,
            bar,
            fill_top,
            fill_bottom,
            stroke,
            [0.0, 0.0, 0.0, 0.0],
            9.0,
            1.0,
            1.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        let prefix = match self.status.kind {
            StatusKind::Neutral => "\u{25CF}",  // ●
            StatusKind::Success => "\u{2713}",  // ✓
            StatusKind::Error => "\u{2717}",    // ✗
            StatusKind::Prompt => "\u{25B6}",   // ▶
        };
        let max_chars = ((bar[2] - self.ui(18.0)) / self.cell_width).max(1.0) as usize;
        let prefix_chars = prefix.chars().count() + 2;
        let text_message =
            compact_status_message(&self.status.message, max_chars.saturating_sub(prefix_chars));
        let status_text = format!("{prefix}  {text_message}");

        let mut x = bar[0] + self.ui(10.0);
        let baseline = bar[1] + (bar[3] - self.line_height) * 0.5 + self.ascent;
        self.append_text_run(text_vertices, &mut x, baseline, &status_text, text)?;

        Ok(())
    }

    fn modal_panel_rect(&self, height: f32) -> [f32; 4] {
        let w = (self.size.width as f32 - self.ui(32.0))
            .min(self.ui(880.0))
            .max(self.ui(320.0));
        let h = height.max(1.0);
        let x = ((self.size.width as f32 - w) * 0.5).max(self.ui(16.0));
        let bottom = self.status_bar_rect()[1] - self.ui(12.0);
        let y = (bottom - h)
            .max(self.toolbar_bar_rect()[1] + self.toolbar_bar_rect()[3] + self.ui(12.0));
        [x, y, w, h]
    }

    fn build_modal_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        if matches!(self.input_mode, InputMode::Normal) {
            return Ok(());
        }

        // Full-screen dimming scrim behind all modals — opaque enough
        // that the underlying chrome reads as backgrounded rather than
        // competing with the modal for attention.
        let w = self.size.width as f32;
        let h = self.size.height as f32;
        push_styled_rect(
            rect_instances,
            [0.0, 0.0, w, h],
            [0.0, 0.0, 0.0, 0.72],
            [0.0, 0.0, 0.0, 0.80],
            [0.0; 4],
            [0.0; 4],
            0.0,
            0.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        match self.input_mode {
            InputMode::CommitSummary | InputMode::CommitBody => {
                self.build_commit_overlay_geometry(text_vertices, rect_instances)
            }
            InputMode::RepoPicker => {
                self.build_repo_picker_overlay_geometry(text_vertices, rect_instances)
            }
            InputMode::DiscardConfirm => {
                self.build_discard_overlay_geometry(text_vertices, rect_instances)
            }
            InputMode::BranchSwitcher => {
                self.build_branch_switcher_overlay_geometry(text_vertices, rect_instances)
            }
            InputMode::Settings => {
                self.build_settings_overlay_geometry(text_vertices, rect_instances)
            }
            InputMode::Normal => Ok(()),
        }
    }

    fn build_commit_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let panel = self.modal_panel_rect(self.ui(134.0));
        push_styled_rect_glow(
            rect_instances,
            panel,
            theme::palette().modal_bg_top,
            theme::palette().modal_bg_bottom,
            theme::palette().modal_border,
            [0.0, 0.0, 0.0, 0.28],
            self.ui(12.0),
            1.0,
            1.0,
            self.ui(16.0),
            [0.0, self.ui(4.0)],
            self.ui(2.0),
            0.25,
        );

        let mut x = panel[0] + self.ui(16.0);
        let mut y = panel[1] + self.ui(18.0);
        self.append_text_run(
            text_vertices,
            &mut x,
            y + self.ascent,
            "Commit message",
            [0.92, 0.96, 1.0, 1.0],
        )?;

        y += self.line_height * 1.2;
        let summary_active = matches!(self.input_mode, InputMode::CommitSummary);
        let body_active = matches!(self.input_mode, InputMode::CommitBody);

        let summary_fill = if summary_active {
            ([0.25, 0.31, 0.47, 1.0], [0.19, 0.24, 0.37, 1.0])
        } else {
            ([0.17, 0.20, 0.28, 1.0], [0.13, 0.16, 0.22, 1.0])
        };
        let body_fill = if body_active {
            ([0.25, 0.31, 0.47, 1.0], [0.19, 0.24, 0.37, 1.0])
        } else {
            ([0.17, 0.20, 0.28, 1.0], [0.13, 0.16, 0.22, 1.0])
        };

        let field_w = panel[2] - self.ui(32.0);
        let summary_rect = [
            panel[0] + self.ui(16.0),
            y,
            field_w,
            self.line_height + self.ui(10.0),
        ];
        let body_rect = [
            panel[0] + self.ui(16.0),
            y + self.line_height + self.ui(18.0),
            field_w,
            self.line_height + self.ui(10.0),
        ];
        push_styled_rect(
            rect_instances,
            summary_rect,
            summary_fill.0,
            summary_fill.1,
            [0.35, 0.47, 0.71, 0.50],
            [0.0, 0.0, 0.0, 0.0],
            self.ui(9.0),
            1.0,
            1.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );
        push_styled_rect(
            rect_instances,
            body_rect,
            body_fill.0,
            body_fill.1,
            [0.35, 0.47, 0.71, 0.50],
            [0.0, 0.0, 0.0, 0.0],
            self.ui(9.0),
            1.0,
            1.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        let max_chars = (field_w / self.cell_width).max(1.0) as usize;
        let mut sx = panel[0] + self.ui(28.0);
        let sy = summary_rect[1] + self.ui(8.0);
        self.append_text_run(
            text_vertices,
            &mut sx,
            sy + self.ascent - self.ui(2.0),
            &format!(
                "Summary: {}",
                compact_status_message(&self.commit_summary, max_chars.saturating_sub(9))
            ),
            [0.94, 0.97, 1.0, 1.0],
        )?;
        let mut bx = panel[0] + self.ui(28.0);
        let by = body_rect[1] + self.ui(8.0);
        self.append_text_run(
            text_vertices,
            &mut bx,
            by + self.ascent - self.ui(2.0),
            &format!(
                "Body: {}",
                compact_status_message(&self.commit_body, max_chars.saturating_sub(6))
            ),
            [0.94, 0.97, 1.0, 1.0],
        )?;

        let mut hint_x = panel[0] + self.ui(16.0);
        let hint_y = panel[1] + panel[3] - self.line_height + self.ui(4.0);
        self.append_text_run(
            text_vertices,
            &mut hint_x,
            hint_y + self.ascent,
            "Enter next / submit  Tab switch  Esc cancel",
            [0.76, 0.82, 0.92, 1.0],
        )?;

        Ok(())
    }

    fn build_repo_picker_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let visible_start = self.repo_picker_scroll.min(self.recent_repos.len());
        let visible_end = (visible_start + 6).min(self.recent_repos.len());
        let visible_count = visible_end.saturating_sub(visible_start);
        let panel_h = self.ui(92.0) + visible_count as f32 * (self.line_height + self.ui(6.0));
        let panel = self.modal_panel_rect(panel_h);
        push_styled_rect_glow(
            rect_instances,
            panel,
            theme::palette().modal_bg_top,
            theme::palette().modal_bg_bottom,
            theme::palette().modal_border,
            [0.0, 0.0, 0.0, 0.28],
            self.ui(12.0),
            1.0,
            1.0,
            self.ui(16.0),
            [0.0, self.ui(4.0)],
            self.ui(2.0),
            0.25,
        );

        let mut x = panel[0] + self.ui(16.0);
        let mut y = panel[1] + self.ui(18.0);
        self.append_text_run(
            text_vertices,
            &mut x,
            y + self.ascent,
            "Recent repositories",
            [0.92, 0.96, 1.0, 1.0],
        )?;

        y += self.line_height * 1.4;
        let row_x = panel[0] + self.ui(14.0);
        let row_w = panel[2] - self.ui(28.0);
        let visible_repos: Vec<PathBuf> = self.recent_repos[visible_start..visible_end].to_vec();
        for (offset, repo) in visible_repos.iter().enumerate() {
            let idx = visible_start + offset;
            let row_y = y + offset as f32 * (self.line_height + self.ui(6.0));
            let selected = idx == self.repo_picker_index;
            let (fill_top, fill_bottom, stroke) = if selected {
                (
                    [0.25, 0.33, 0.50, 1.0],
                    [0.18, 0.24, 0.38, 1.0],
                    [0.52, 0.68, 0.98, 0.70],
                )
            } else {
                (
                    [0.16, 0.18, 0.26, 1.0],
                    [0.12, 0.14, 0.20, 1.0],
                    [0.30, 0.36, 0.48, 0.40],
                )
            };
            push_styled_rect(
                rect_instances,
                [row_x, row_y, row_w, self.line_height + self.ui(4.0)],
                fill_top,
                fill_bottom,
                stroke,
                [0.0, 0.0, 0.0, 0.0],
                self.ui(8.0),
                1.0,
                1.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            let mut tx = row_x + self.ui(10.0);
            let mut label = repo.display().to_string();
            if repo == self.git.repo_root() {
                label.push_str("  (current)");
            }
            self.append_text_run(
                text_vertices,
                &mut tx,
                row_y + self.ascent + 2.0,
                &compact_status_message(
                    &label,
                    ((row_w - self.ui(20.0)) / self.cell_width).max(1.0) as usize,
                ),
                if selected {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    [0.86, 0.90, 0.96, 1.0]
                },
            )?;
        }

        let mut hint_x = panel[0] + self.ui(16.0);
        let hint_y = panel[1] + panel[3] - self.line_height + self.ui(4.0);
        self.append_text_run(
            text_vertices,
            &mut hint_x,
            hint_y + self.ascent,
            "Enter open  Esc cancel  Up/Down move",
            [0.76, 0.82, 0.92, 1.0],
        )?;

        Ok(())
    }

    fn build_discard_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let panel = self.modal_panel_rect(self.ui(108.0));
        push_styled_rect_glow(
            rect_instances,
            panel,
            theme::palette().modal_danger_bg_top,
            theme::palette().modal_danger_bg_bottom,
            theme::palette().modal_danger_border,
            [0.0, 0.0, 0.0, 0.32],
            self.ui(12.0),
            1.0,
            1.0,
            self.ui(16.0),
            [0.0, self.ui(4.0)],
            self.ui(2.0),
            0.25,
        );

        let mut x = panel[0] + self.ui(16.0);
        let y = panel[1] + self.ui(18.0);
        self.append_text_run(
            text_vertices,
            &mut x,
            y + self.ascent,
            "Discard selected file?",
            [1.0, 0.94, 0.94, 1.0],
        )?;

        if let Some(path) = &self.pending_discard_path {
            let mut px = panel[0] + self.ui(16.0);
            let py = panel[1] + self.line_height * 1.6;
            self.append_text_run(
                text_vertices,
                &mut px,
                py + self.ascent,
                &compact_status_message(
                    &path.display().to_string(),
                    ((panel[2] - self.ui(32.0)) / self.cell_width).max(1.0) as usize,
                ),
                [1.0, 0.86, 0.86, 1.0],
            )?;
        }

        let mut hint_x = panel[0] + self.ui(16.0);
        let hint_y = panel[1] + panel[3] - self.line_height + self.ui(4.0);
        self.append_text_run(
            text_vertices,
            &mut hint_x,
            hint_y + self.ascent,
            "Enter / y confirm  Esc cancel",
            [1.0, 0.84, 0.84, 1.0],
        )?;

        Ok(())
    }

    fn build_branch_switcher_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let visible_start = self.branch_picker_scroll.min(self.branch_list.len());
        let visible_end = (visible_start + 6).min(self.branch_list.len());
        let visible_count = visible_end.saturating_sub(visible_start);
        let panel_h = self.ui(92.0) + visible_count as f32 * (self.line_height + self.ui(6.0));
        let panel = self.modal_panel_rect(panel_h);
        push_styled_rect_glow(
            rect_instances,
            panel,
            theme::palette().modal_bg_top,
            theme::palette().modal_bg_bottom,
            theme::palette().modal_border,
            [0.0, 0.0, 0.0, 0.28],
            self.ui(12.0),
            1.0,
            1.0,
            self.ui(16.0),
            [0.0, self.ui(4.0)],
            self.ui(2.0),
            0.25,
        );

        let mut x = panel[0] + self.ui(16.0);
        let mut y = panel[1] + self.ui(18.0);
        self.append_text_run(
            text_vertices,
            &mut x,
            y + self.ascent,
            "Switch branch",
            [0.92, 0.96, 1.0, 1.0],
        )?;

        y += self.line_height * 1.4;
        let row_x = panel[0] + self.ui(14.0);
        let row_w = panel[2] - self.ui(28.0);
        let visible_branches: Vec<String> = self.branch_list[visible_start..visible_end].to_vec();
        for (offset, branch) in visible_branches.iter().enumerate() {
            let idx = visible_start + offset;
            let row_y = y + offset as f32 * (self.line_height + self.ui(6.0));
            let selected = idx == self.branch_picker_index;
            let is_current = branch == &self.branch_current;
            let (fill_top, fill_bottom, stroke) = if selected {
                (
                    [0.25, 0.33, 0.50, 1.0],
                    [0.18, 0.24, 0.38, 1.0],
                    [0.52, 0.68, 0.98, 0.70],
                )
            } else {
                (
                    [0.16, 0.18, 0.26, 1.0],
                    [0.12, 0.14, 0.20, 1.0],
                    [0.30, 0.36, 0.48, 0.40],
                )
            };
            push_styled_rect(
                rect_instances,
                [row_x, row_y, row_w, self.line_height + self.ui(4.0)],
                fill_top,
                fill_bottom,
                stroke,
                [0.0, 0.0, 0.0, 0.0],
                self.ui(8.0),
                1.0,
                1.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            let mut tx = row_x + self.ui(10.0);
            let mut label = branch.clone();
            if is_current {
                label.push_str("  (current)");
            }
            let text_color = if is_current && !selected {
                theme::palette().branch_current_badge
            } else if selected {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.86, 0.90, 0.96, 1.0]
            };
            self.append_text_run(
                text_vertices,
                &mut tx,
                row_y + self.ascent + 2.0,
                &compact_status_message(
                    &label,
                    ((row_w - self.ui(20.0)) / self.cell_width).max(1.0) as usize,
                ),
                text_color,
            )?;
        }

        let mut hint_x = panel[0] + self.ui(16.0);
        let hint_y = panel[1] + panel[3] - self.line_height + self.ui(4.0);
        self.append_text_run(
            text_vertices,
            &mut hint_x,
            hint_y + self.ascent,
            "Enter checkout  Esc cancel  Up/Down move",
            [0.76, 0.82, 0.92, 1.0],
        )?;

        Ok(())
    }

    fn build_settings_overlay_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let row_count = Self::SETTINGS_COUNT;
        let panel_h =
            self.ui(92.0) + row_count as f32 * (self.line_height + self.ui(6.0));
        let panel = self.modal_panel_rect(panel_h);
        push_styled_rect_glow(
            rect_instances,
            panel,
            theme::palette().modal_bg_top,
            theme::palette().modal_bg_bottom,
            theme::palette().modal_border,
            [0.0, 0.0, 0.0, 0.28],
            self.ui(12.0),
            1.0,
            1.0,
            self.ui(16.0),
            [0.0, self.ui(4.0)],
            self.ui(2.0),
            0.25,
        );

        // Title
        let mut x = panel[0] + self.ui(16.0);
        let mut y = panel[1] + self.ui(18.0);
        self.append_text_run(
            text_vertices,
            &mut x,
            y + self.ascent,
            "Settings",
            [0.92, 0.96, 1.0, 1.0],
        )?;

        y += self.line_height * 1.4;
        let row_x = panel[0] + self.ui(14.0);
        let row_w = panel[2] - self.ui(28.0);

        for idx in 0..row_count {
            let row_y = y + idx as f32 * (self.line_height + self.ui(6.0));
            let selected = idx == self.settings_index;

            let (fill_top, fill_bottom, stroke) = if selected {
                (
                    [0.25, 0.33, 0.50, 1.0],
                    [0.18, 0.24, 0.38, 1.0],
                    [0.52, 0.68, 0.98, 0.70],
                )
            } else {
                (
                    [0.16, 0.18, 0.26, 1.0],
                    [0.12, 0.14, 0.20, 1.0],
                    [0.30, 0.36, 0.48, 0.40],
                )
            };

            push_styled_rect(
                rect_instances,
                [row_x, row_y, row_w, self.line_height + self.ui(4.0)],
                fill_top,
                fill_bottom,
                stroke,
                [0.0, 0.0, 0.0, 0.0],
                self.ui(8.0),
                1.0,
                1.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            let (label, value) = self.settings_row_label(idx);
            let arrows = if selected { "<  >  " } else { "      " };
            let display = format!("{label}:  {arrows}{value}");

            let text_color = if selected {
                [1.0, 1.0, 1.0, 1.0]
            } else {
                [0.86, 0.90, 0.96, 1.0]
            };

            let mut tx = row_x + self.ui(10.0);
            self.append_text_run(
                text_vertices,
                &mut tx,
                row_y + self.ascent + 2.0,
                &compact_status_message(
                    &display,
                    ((row_w - self.ui(20.0)) / self.cell_width).max(1.0) as usize,
                ),
                text_color,
            )?;
        }

        // Hint bar
        let mut hint_x = panel[0] + self.ui(16.0);
        let hint_y = panel[1] + panel[3] - self.line_height + self.ui(4.0);
        self.append_text_run(
            text_vertices,
            &mut hint_x,
            hint_y + self.ascent,
            "Left/Right change  Up/Down navigate  Esc close",
            [0.76, 0.82, 0.92, 1.0],
        )?;

        Ok(())
    }

    fn refresh_document_from_git(&mut self) -> anyhow::Result<()> {
        let (file_doc, meta, diff_doc) = self.git.build_split_documents()?;
        let (file_line_to_index, file_index_to_line) =
            build_grouped_file_maps(&file_doc, &meta);
        self.file_doc = file_doc;
        self.diff_doc = diff_doc;
        self.file_line_to_index = file_line_to_index;
        self.file_index_to_line = file_index_to_line;
        // Reset diff scroll when file changes
        self.diff_scroll_y = 0.0;
        self.refresh_repo_tracking()?;
        self.layout_dirty = true;
        self.geometry_dirty = true;
        Ok(())
    }

    fn rebuild_layout(&mut self) {
        let content = self.content_panel_rect();

        // ── File pane layout ──────────────────────────────
        self.file_visual_lines.clear();
        let mut y = content[1] + self.ui(8.0);
        for line_idx in 0..self.file_doc.line_count() {
            self.file_visual_lines.push(VisualLine {
                y_top: y,
                line_index: line_idx,
                style: self.file_doc.line_style(line_idx),
                glyphs: Vec::new(),
                shaped: false,
            });
            y += self.line_height;
        }
        self.file_content_height = (y + self.ui(16.0)).max(content[3]);

        // ── Diff pane layout ──────────────────────────────
        self.diff_visual_lines.clear();
        let mut y = content[1] + self.ui(8.0);
        for line_idx in 0..self.diff_doc.line_count() {
            self.diff_visual_lines.push(VisualLine {
                y_top: y,
                line_index: line_idx,
                style: self.diff_doc.line_style(line_idx),
                glyphs: Vec::new(),
                shaped: false,
            });
            y += self.line_height;
        }
        self.diff_content_height = (y + self.ui(16.0)).max(content[3]);

        self.clamp_scroll();
        self.layout_dirty = false;
        self.geometry_dirty = true;
    }

    fn ensure_file_line_shaped(&mut self, idx: usize) {
        if idx >= self.file_visual_lines.len() || self.file_visual_lines[idx].shaped {
            return;
        }
        let file_pane = self.file_pane_rect();
        let x_start = file_pane[0] + self.ui(8.0);
        self.shape_line_for_pane(idx, &PaneRef::Files, x_start);
    }

    fn ensure_diff_line_shaped(&mut self, idx: usize) {
        if idx >= self.diff_visual_lines.len() || self.diff_visual_lines[idx].shaped {
            return;
        }
        let diff_pane = self.diff_pane_rect();
        let x_start = diff_pane[0] + self.diff_line_number_width() + self.ui(8.0);
        self.shape_line_for_pane(idx, &PaneRef::Diff, x_start);
    }

    fn shape_line_for_pane(&mut self, idx: usize, pane: &PaneRef, x_start: f32) {
        let (doc, visual_lines) = match pane {
            PaneRef::Files => (&self.file_doc, &mut self.file_visual_lines),
            PaneRef::Diff => (&self.diff_doc, &mut self.diff_visual_lines),
        };

        let y_top = visual_lines[idx].y_top;
        let line_index = visual_lines[idx].line_index;
        let style = visual_lines[idx].style;
        let baseline = y_top + self.ascent;

        let mut x = x_start;
        let mut glyphs = Vec::new();
        let line_text = doc.line_text(line_index);
        let spans = doc.line_spans(line_index);

        for (col, ch) in line_text.chars().enumerate() {
            if !ch.is_control() {
                let glyph_id = self.font.glyph_id(ch).0;
                let color = spans
                    .iter()
                    .find(|s| col >= s.start_col && col < s.end_col)
                    .map(|s| s.color)
                    .unwrap_or_else(|| style.color());
                glyphs.push(ShapedGlyph {
                    glyph_id,
                    x,
                    y: baseline,
                    color,
                });
            }
            x += self.cell_width;
        }

        visual_lines[idx].glyphs = glyphs;
        visual_lines[idx].shaped = true;
    }

    fn ensure_glyph(&mut self, glyph_id: u16) -> anyhow::Result<Option<GlyphUV>> {
        let key = GlyphKey {
            glyph_id,
            px_q: (self.font_px() * 64.0) as u16,
        };

        if let Some(uv) = self.glyph_cache.get(&key).copied() {
            return Ok(Some(uv));
        }

        let glyph = Glyph {
            id: GlyphId(glyph_id),
            scale: PxScale::from(self.font_px()),
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

    /// Bundled toolbar icons. Each entry maps a logical icon name to a
    /// raw SVG file embedded at compile time.
    fn icon_sources() -> &'static [(&'static str, &'static str)] {
        &[
            ("plus", include_str!("../assets/icons/plus.svg")),
            ("minus", include_str!("../assets/icons/minus.svg")),
            (
                "plus-circle",
                include_str!("../assets/icons/plus-circle.svg"),
            ),
            (
                "minus-circle",
                include_str!("../assets/icons/minus-circle.svg"),
            ),
            (
                "arrow-up",
                include_str!("../assets/icons/arrow-narrow-up.svg"),
            ),
            (
                "arrow-down",
                include_str!("../assets/icons/arrow-narrow-down.svg"),
            ),
            ("download", include_str!("../assets/icons/download-01.svg")),
            ("commit", include_str!("../assets/icons/git-commit.svg")),
            ("trash", include_str!("../assets/icons/trash-02.svg")),
            ("folder", include_str!("../assets/icons/folder.svg")),
            (
                "folder-closed",
                include_str!("../assets/icons/folder-closed.svg"),
            ),
            (
                "git-branch",
                include_str!("../assets/icons/git-branch-02.svg"),
            ),
            (
                "refresh",
                include_str!("../assets/icons/refresh-ccw-01.svg"),
            ),
            ("settings", include_str!("../assets/icons/settings.svg")),
            ("close", include_str!("../assets/icons/x-close.svg")),
        ]
    }

    /// Rasterize every bundled icon at the current DPI and upload each
    /// into the shared glyph atlas. Called once at startup.
    fn load_toolbar_icons(&mut self) -> anyhow::Result<()> {
        // Render at the same nominal cell size we'll display at, so we
        // don't blur from scale-up. 18 px nominal × ui_scale matches the
        // toolbar text height.
        let target = (18.0 * self.ui_scale).round().max(8.0) as u32;
        let pad = 1u32;

        for (name, svg) in Self::icon_sources() {
            let Some(d) = crate::icon::extract_path_d(svg) else {
                continue;
            };
            let (_vx, _vy, vw, vh) = crate::icon::extract_viewbox(svg);
            let path = crate::icon::parse_path(d);

            let alpha = crate::icon::rasterize_filled(&path, vw, vh, target, target);

            let gw = target + pad * 2;
            let gh = target + pad * 2;
            let (x, y) = self
                .atlas
                .alloc(gw, gh)
                .context("icon atlas full while loading toolbar icons")?;

            let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
            let bpr = gw.next_multiple_of(align);
            let mut tmp = vec![0u8; (bpr * gh) as usize];
            for row in 0..target {
                let src = (row * target) as usize;
                let dst = ((row + pad) * bpr + pad) as usize;
                tmp[dst..dst + target as usize]
                    .copy_from_slice(&alpha[src..src + target as usize]);
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
                u1: (x + pad + target) as f32 / self.atlas.w as f32,
                v1: (y + pad + target) as f32 / self.atlas.h as f32,
                w: target,
                h: target,
                bearing_x: 0.0,
                bearing_y: 0.0,
            };
            self.icons.insert(name, uv);
        }

        Ok(())
    }

    /// Emit a textured quad for the named icon at logical (x, y) of the
    /// given pixel size, tinted by `color`. Same single-channel R8 +
    /// color path as text rendering, so it goes straight into the
    /// existing text pipeline.
    fn append_icon(
        &self,
        out: &mut Vec<TextVertex>,
        x: f32,
        y: f32,
        size: f32,
        name: &str,
        color: [f32; 4],
    ) {
        let Some(uv) = self.icons.get(name).copied() else {
            return;
        };
        let x0 = x.round();
        let y0 = y.round();
        let x1 = x0 + size;
        let y1 = y0 + size;
        out.push(TextVertex {
            pos: [x0, y0],
            uv: [uv.u0, uv.v0],
            color,
        });
        out.push(TextVertex {
            pos: [x1, y0],
            uv: [uv.u1, uv.v0],
            color,
        });
        out.push(TextVertex {
            pos: [x0, y1],
            uv: [uv.u0, uv.v1],
            color,
        });
        out.push(TextVertex {
            pos: [x0, y1],
            uv: [uv.u0, uv.v1],
            color,
        });
        out.push(TextVertex {
            pos: [x1, y0],
            uv: [uv.u1, uv.v0],
            color,
        });
        out.push(TextVertex {
            pos: [x1, y1],
            uv: [uv.u1, uv.v1],
            color,
        });
    }

    fn selected_file_line_index(&self) -> Option<usize> {
        self.file_index_to_line
            .get(self.git.selected_index())
            .copied()
    }

    fn titlebar_rect(&self) -> [f32; 4] {
        [0.0, 0.0, self.size.width as f32, self.ui(34.0)]
    }

    fn toolbar_bar_rect(&self) -> [f32; 4] {
        let t = self.titlebar_rect();
        [t[0], t[1] + t[3], t[2], self.ui(42.0)]
    }

    fn content_panel_rect(&self) -> [f32; 4] {
        let tb = self.toolbar_bar_rect();
        let status = self.status_bar_rect_raw();
        let top = tb[1] + tb[3];
        let bottom = status[1] - self.status_bar_gap();
        [0.0, top, self.size.width as f32, (bottom - top).max(1.0)]
    }

    /// Raw status bar rect without depending on content_panel_rect (avoids recursion)
    fn status_bar_rect_raw(&self) -> [f32; 4] {
        let x = self.status_bar_side_padding();
        let y = self.size.height as f32 - self.status_bar_height() - self.status_bar_gap();
        let w = (self.size.width as f32 - self.status_bar_side_padding() * 2.0).max(1.0);
        [x, y, w, self.status_bar_height()]
    }

    /// File pane (left) — uses adjustable ratio of content width
    fn file_pane_rect(&self) -> [f32; 4] {
        let content = self.content_panel_rect();
        let pane_w = (content[2] * self.file_pane_ratio)
            .max(self.ui(120.0))
            .min(content[2] * 0.70);
        [content[0], content[1], pane_w, content[3]]
    }

    /// Diff pane (right) — remaining width
    fn diff_pane_rect(&self) -> [f32; 4] {
        let content = self.content_panel_rect();
        let file_pane = self.file_pane_rect();
        let divider_w = self.ui(1.0);
        let diff_x = file_pane[0] + file_pane[2] + divider_w;
        let diff_w = (content[0] + content[2] - diff_x).max(1.0);
        [diff_x, content[1], diff_w, content[3]]
    }

    /// Width reserved for line numbers in diff pane
    fn diff_line_number_width(&self) -> f32 {
        // "9999 9999 " = 10 chars
        self.cell_width * 10.0
    }

    fn append_text_run(
        &mut self,
        out: &mut Vec<TextVertex>,
        x: &mut f32,
        baseline: f32,
        text: &str,
        color: [f32; 4],
    ) -> anyhow::Result<()> {
        for ch in text.chars() {
            if !ch.is_control() {
                let uv = match self.ensure_glyph(self.font.glyph_id(ch).0)? {
                    Some(uv) => uv,
                    None => {
                        *x += self.cell_width;
                        continue;
                    }
                };

                if uv.w > 0 && uv.h > 0 {
                    let x0 = (*x + uv.bearing_x).round();
                    let y0 = (baseline + uv.bearing_y).round();
                    let x1 = x0 + uv.w as f32;
                    let y1 = y0 + uv.h as f32;

                    out.push(TextVertex {
                        pos: [x0, y0],
                        uv: [uv.u0, uv.v0],
                        color,
                    });
                    out.push(TextVertex {
                        pos: [x1, y0],
                        uv: [uv.u1, uv.v0],
                        color,
                    });
                    out.push(TextVertex {
                        pos: [x0, y1],
                        uv: [uv.u0, uv.v1],
                        color,
                    });
                    out.push(TextVertex {
                        pos: [x0, y1],
                        uv: [uv.u0, uv.v1],
                        color,
                    });
                    out.push(TextVertex {
                        pos: [x1, y0],
                        uv: [uv.u1, uv.v0],
                        color,
                    });
                    out.push(TextVertex {
                        pos: [x1, y1],
                        uv: [uv.u1, uv.v1],
                        color,
                    });
                }
            }
            *x += self.cell_width;
        }
        Ok(())
    }

    fn build_titlebar_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        self.window_controls.clear();
        let bar = self.titlebar_rect();
        let bx = bar[0];
        let by = bar[1];
        let bw = bar[2];
        let bh = bar[3];

        // Titlebar background — subtle top-edge chrome lift
        push_styled_rect_glow(
            rect_instances,
            bar,
            theme::palette().titlebar_top,
            theme::palette().titlebar_bottom,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            0.0,
            1.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
            0.20,
        );

        // Bottom divider line for titlebar
        push_styled_rect(
            rect_instances,
            [bx, by + bh - 1.0, bw, 1.0],
            theme::palette().divider,
            theme::palette().divider,
            [0.0; 4],
            [0.0; 4],
            0.0,
            0.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        // On macOS we let the native window draw its real traffic lights
        // (via NSWindow + titlebar_transparent + fullsize_content_view).
        // We just paint our titlebar bg behind them and leave clearance.
        // On other platforms we still draw our own controls.
        #[cfg(not(target_os = "macos"))]
        {
            let controls = [
                (
                    WindowControlAction::Close,
                    [0.95, 0.41, 0.38, 0.96],
                    [0.77, 0.26, 0.24, 0.96],
                ),
                (
                    WindowControlAction::Minimize,
                    [0.98, 0.78, 0.41, 0.96],
                    [0.84, 0.63, 0.25, 0.96],
                ),
                (
                    WindowControlAction::Zoom,
                    [0.40, 0.83, 0.49, 0.96],
                    [0.26, 0.68, 0.34, 0.96],
                ),
            ];

            let mut cx = bx + self.ui(14.0);
            let cy = by + bh * 0.5;
            let d = self.ui(12.0);
            for (action, top, bottom) in controls {
                let x0 = cx - d * 0.5;
                let y0 = cy - d * 0.5;
                push_styled_rect(
                    rect_instances,
                    [x0, y0, d, d],
                    top,
                    bottom,
                    [0.0, 0.0, 0.0, 0.22],
                    [0.0, 0.0, 0.0, 0.18],
                    self.ui(6.0),
                    1.0,
                    self.ui(0.8),
                    self.ui(2.0),
                    [0.0, self.ui(0.5)],
                    0.0,
                );
                self.window_controls.push(WindowControlButton {
                    x0,
                    y0,
                    x1: x0 + d,
                    y1: y0 + d,
                    action,
                });
                cx += self.ui(18.0);
            }
        }

        // Build titlebar content: "wgit" brand + repo path + branch badge.
        // macOS native traffic lights occupy roughly the first 80 logical
        // px of the titlebar, so we clear them with an extra inset there.
        let baseline = by + (bh - self.line_height) * 0.5 + self.ascent;
        let brand_inset = if cfg!(target_os = "macos") {
            self.ui(86.0)
        } else {
            self.ui(68.0)
        };
        let mut x = bx + brand_inset;

        // Brand name
        self.append_text_run(
            text_vertices,
            &mut x,
            baseline,
            "wgit",
            theme::palette().text_accent,
        )?;
        x += self.cell_width;

        // Repo path (dimmed)
        let repo_name = self
            .git
            .repo_root()
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.append_text_run(
            text_vertices,
            &mut x,
            baseline,
            &repo_name,
            theme::palette().text_secondary,
        )?;
        x += self.cell_width * 2.0;

        // Branch badge with background chip.
        // We render the text first to capture its actual advance, then
        // push the chip rect sized from that. The render pass draws all
        // rects before any text, so the chip still appears *behind* the
        // glyphs visually even though it's pushed afterwards. This way
        // the chip can never desync from the text width.
        // The Powerline branch glyph (`\u{E0A0}`) isn't present in the
        // bundled Hack font, so prefixing it would consume two cells of
        // layout advance while drawing nothing — leaving a phantom gap
        // on the left of the chip. Skip the prefix; the chip's
        // placement is identification enough.
        let branch = self.repo_tracking.branch.trim().to_string();
        let branch_label = branch.clone();
        let chip_h = self.line_height - self.ui(4.0);
        let chip_y = by + (bh - chip_h) * 0.5;
        let pad_x = self.ui(8.0);

        let text_x_start = x;
        self.append_text_run(
            text_vertices,
            &mut x,
            baseline,
            &branch_label,
            theme::palette().accent_blue,
        )?;
        let text_advance = x - text_x_start;

        let chip_x = (text_x_start - pad_x).round();
        let chip_w = (text_advance + pad_x * 2.0).round();

        push_styled_rect_glow(
            rect_instances,
            [chip_x, chip_y, chip_w, chip_h],
            theme::palette().branch_chip_bg_top,
            theme::palette().branch_chip_bg_bottom,
            theme::palette().accent_blue_dim,
            [0.0; 4],
            self.ui(5.0),
            1.0,
            1.0,
            0.0,
            [0.0, 0.0],
            0.0,
            0.30,
        );

        x += self.cell_width;

        // Ahead/behind indicators
        if self.repo_tracking.ahead > 0 {
            let ahead_text = format!("\u{2191}{}", self.repo_tracking.ahead);
            self.append_text_run(
                text_vertices,
                &mut x,
                baseline,
                &ahead_text,
                theme::palette().accent_green,
            )?;
            x += self.cell_width;
        }
        if self.repo_tracking.behind > 0 {
            let behind_text = format!("\u{2193}{}", self.repo_tracking.behind);
            self.append_text_run(
                text_vertices,
                &mut x,
                baseline,
                &behind_text,
                theme::palette().accent_red,
            )?;
        };

        Ok(())
    }

    fn toolbar_button_configs(&self) -> Vec<ButtonConfig> {
        let green_btn = ButtonStyle {
            fill_top: [0.16, 0.28, 0.20, 0.50],
            fill_bottom: [0.12, 0.22, 0.16, 0.45],
            stroke: [0.36, 0.68, 0.46, 0.50],
            text: theme::palette().accent_green,
        };
        let blue_btn = ButtonStyle {
            fill_top: [0.16, 0.22, 0.36, 0.50],
            fill_bottom: [0.12, 0.17, 0.28, 0.45],
            stroke: [0.36, 0.50, 0.78, 0.50],
            text: theme::palette().accent_blue,
        };
        let purple_btn = ButtonStyle {
            fill_top: [0.22, 0.17, 0.34, 0.50],
            fill_bottom: [0.17, 0.13, 0.26, 0.45],
            stroke: [0.50, 0.40, 0.78, 0.50],
            text: theme::palette().accent_purple,
        };
        let yellow_btn = ButtonStyle {
            fill_top: [0.28, 0.24, 0.14, 0.50],
            fill_bottom: [0.22, 0.18, 0.10, 0.45],
            stroke: [0.68, 0.56, 0.30, 0.50],
            text: theme::palette().accent_yellow,
        };
        let red_btn = ButtonStyle {
            fill_top: [0.32, 0.14, 0.14, 0.60],
            fill_bottom: [0.26, 0.10, 0.10, 0.55],
            stroke: [0.78, 0.34, 0.34, 0.60],
            text: theme::palette().accent_red,
        };
        let gray_btn = ButtonStyle {
            fill_top: [0.18, 0.18, 0.20, 0.40],
            fill_bottom: [0.14, 0.14, 0.16, 0.35],
            stroke: [0.40, 0.40, 0.44, 0.35],
            text: theme::palette().text_secondary,
        };

        vec![
            // ── Staging group ─────────────────────
            ButtonConfig {
                label: String::from("Stage (s)"),
                icon: "plus",
                action: ToolbarAction::Stage,
                group: ToolbarGroup::Staging,
                style: green_btn,
            },
            ButtonConfig {
                label: String::from("Stage all (a)"),
                icon: "plus-circle",
                action: ToolbarAction::StageAll,
                group: ToolbarGroup::Staging,
                style: green_btn,
            },
            ButtonConfig {
                label: String::from("Unstage (u)"),
                icon: "minus",
                action: ToolbarAction::Unstage,
                group: ToolbarGroup::Staging,
                style: yellow_btn,
            },
            ButtonConfig {
                label: String::from("Unstage all (U)"),
                icon: "minus-circle",
                action: ToolbarAction::UnstageAll,
                group: ToolbarGroup::Staging,
                style: yellow_btn,
            },
            // ── Git ops group ─────────────────────
            ButtonConfig {
                label: String::from("Commit (c)"),
                icon: "commit",
                action: ToolbarAction::Commit,
                group: ToolbarGroup::GitOps,
                style: blue_btn,
            },
            ButtonConfig {
                label: String::from("Fetch (f)"),
                icon: "download",
                action: ToolbarAction::Fetch,
                group: ToolbarGroup::GitOps,
                style: purple_btn,
            },
            ButtonConfig {
                label: String::from("Pull (p)"),
                icon: "arrow-down",
                action: ToolbarAction::Pull,
                group: ToolbarGroup::GitOps,
                style: purple_btn,
            },
            ButtonConfig {
                label: String::from("Push (P)"),
                icon: "arrow-up",
                action: ToolbarAction::Push,
                group: ToolbarGroup::GitOps,
                style: purple_btn,
            },
            // ── Danger group ──────────────────────
            ButtonConfig {
                label: String::from("Discard (x)"),
                icon: "trash",
                action: ToolbarAction::Discard,
                group: ToolbarGroup::Danger,
                style: red_btn,
            },
            // ── App group ─────────────────────────
            ButtonConfig {
                label: String::from("Repos (o)"),
                icon: "folder-closed",
                action: ToolbarAction::RepoSwitch,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
            ButtonConfig {
                label: String::from("Browse (b)"),
                icon: "folder",
                action: ToolbarAction::Browse,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
            ButtonConfig {
                label: String::from("Branch (B)"),
                icon: "git-branch",
                action: ToolbarAction::BranchSwitch,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
            ButtonConfig {
                label: String::from("Refresh (r)"),
                icon: "refresh",
                action: ToolbarAction::Refresh,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
            ButtonConfig {
                label: String::from("Settings (,)"),
                icon: "settings",
                action: ToolbarAction::Settings,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
            ButtonConfig {
                label: String::from("Quit (q)"),
                icon: "close",
                action: ToolbarAction::Quit,
                group: ToolbarGroup::App,
                style: gray_btn,
            },
        ]
    }

    fn build_toolbar_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        self.toolbar_buttons.clear();

        let bar = self.toolbar_bar_rect();
        let bx = bar[0];
        let by = bar[1];
        let bw = bar[2];
        let bh = bar[3];

        // Toolbar background — barely-there lift, weaker than titlebar
        push_styled_rect_glow(
            rect_instances,
            bar,
            theme::palette().toolbar_top,
            theme::palette().toolbar_bottom,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 0.0],
            0.0,
            1.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
            0.12,
        );

        // Bottom divider
        push_styled_rect(
            rect_instances,
            [bx, by + bh - 1.0, bw, 1.0],
            theme::palette().divider,
            theme::palette().divider,
            [0.0; 4],
            [0.0; 4],
            0.0,
            0.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        let mut x = bx + self.ui(14.0);
        let text_max_x = bx + bw - self.ui(14.0);

        let buttons = self.toolbar_button_configs();
        let mut prev_group: Option<ToolbarGroup> = None;
        let icon_px = self.ui(18.0); // logical icon size
        let chip_pad = self.ui(7.0); // padding around the icon, each side
        let chip_w = icon_px + chip_pad * 2.0;
        let chip_h = (bh - self.ui(14.0)).max(icon_px + chip_pad);
        let chip_gap = self.ui(4.0);
        let group_gap = self.ui(14.0);

        for button in buttons {
            if let Some(pg) = prev_group {
                if pg != button.group {
                    let sep_x = x + (group_gap * 0.5);
                    let sep_y = by + self.ui(12.0);
                    let sep_h = bh - self.ui(24.0);
                    push_styled_rect(
                        rect_instances,
                        [sep_x, sep_y, 1.0, sep_h],
                        theme::palette().toolbar_separator,
                        theme::palette().toolbar_separator,
                        [0.0; 4],
                        [0.0; 4],
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        [0.0, 0.0],
                        0.0,
                    );
                    x += group_gap;
                } else {
                    x += chip_gap;
                }
            }

            if x + chip_w > text_max_x {
                break;
            }

            let chip_x0 = x;
            let chip_y0 = by + (bh - chip_h) * 0.5;

            push_styled_rect(
                rect_instances,
                [chip_x0, chip_y0, chip_w, chip_h],
                button.style.fill_top,
                button.style.fill_bottom,
                button.style.stroke,
                [0.0, 0.0, 0.0, 0.0],
                self.ui(6.0),
                1.0,
                1.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            self.toolbar_buttons.push(ToolbarButton {
                x0: chip_x0,
                y0: chip_y0,
                x1: chip_x0 + chip_w,
                y1: chip_y0 + chip_h,
                action: button.action,
            });

            // Center the icon in the chip
            let icon_x = chip_x0 + (chip_w - icon_px) * 0.5;
            let icon_y = chip_y0 + (chip_h - icon_px) * 0.5;
            self.append_icon(
                text_vertices,
                icon_x,
                icon_y,
                icon_px,
                button.icon,
                button.style.text,
            );

            x = chip_x0 + chip_w;
            prev_group = Some(button.group);
        }

        Ok(())
    }

    fn build_tooltip_geometry(
        &mut self,
        text_vertices: &mut Vec<TextVertex>,
        rect_instances: &mut Vec<StyledRectInstance>,
    ) -> anyhow::Result<()> {
        let Some(action) = self.hover_toolbar_action else {
            return Ok(());
        };
        let Some(started) = self.hover_started_at else {
            return Ok(());
        };
        if started.elapsed() < TOOLTIP_DELAY {
            return Ok(());
        }

        let Some(button) = self
            .toolbar_buttons
            .iter()
            .find(|b| b.action == action)
            .copied()
        else {
            return Ok(());
        };

        let Some(label) = self
            .toolbar_button_configs()
            .into_iter()
            .find(|c| c.action == action)
            .map(|c| c.label)
        else {
            return Ok(());
        };

        let pad_x = self.ui(8.0);
        let pad_y = self.ui(4.0);
        let text_w = label.chars().count() as f32 * self.cell_width;
        let text_h = self.line_height;
        let tip_w = text_w + pad_x * 2.0;
        let tip_h = text_h + pad_y * 2.0;
        let gap = self.ui(6.0);

        let btn_cx = (button.x0 + button.x1) * 0.5;
        let mut tip_x = (btn_cx - tip_w * 0.5).round();
        let tip_y = (button.y1 + gap).round();

        let screen_w = self.size.width as f32;
        let margin = self.ui(4.0);
        if tip_x < margin {
            tip_x = margin;
        }
        if tip_x + tip_w > screen_w - margin {
            tip_x = (screen_w - margin - tip_w).max(margin);
        }

        let bg_top = theme::palette().tooltip_bg_top;
        let bg_bottom = theme::palette().tooltip_bg_bottom;
        let border = theme::palette().tooltip_border;
        let text_color = theme::palette().tooltip_text;

        push_styled_rect_glow(
            rect_instances,
            [tip_x, tip_y, tip_w, tip_h],
            bg_top,
            bg_bottom,
            border,
            [0.0, 0.0, 0.0, 0.45],
            self.ui(5.0),
            1.0,
            1.0,
            self.ui(10.0),
            [0.0, self.ui(3.0)],
            self.ui(1.0),
            0.0,
        );

        let baseline = tip_y + pad_y + self.ascent;
        let mut x = (tip_x + pad_x).round();
        self.append_text_run(text_vertices, &mut x, baseline, &label, text_color)?;

        self.tooltip_drawn = true;
        Ok(())
    }

    fn window_control_action_at(&self, pos: PhysicalPosition<f64>) -> Option<WindowControlAction> {
        let x = pos.x as f32;
        let y = pos.y as f32;
        self.window_controls
            .iter()
            .find(|b| x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1)
            .map(|b| b.action)
    }

    fn toolbar_action_at(&self, pos: PhysicalPosition<f64>) -> Option<ToolbarAction> {
        let x = pos.x as f32;
        let y = pos.y as f32;
        self.toolbar_buttons
            .iter()
            .find(|b| x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1)
            .map(|b| b.action)
    }

    fn is_in_titlebar_drag_region(&self, pos: PhysicalPosition<f64>) -> bool {
        let x = pos.x as f32;
        let y = pos.y as f32;
        let t = self.titlebar_rect();
        let in_bar = x >= t[0] && x <= t[0] + t[2] && y >= t[1] && y <= t[1] + t[3];
        if !in_bar {
            return false;
        }
        self.window_control_action_at(pos).is_none() && self.toolbar_action_at(pos).is_none()
    }

    fn rebuild_visible_geometry(&mut self) -> anyhow::Result<()> {
        let mut text_vertices = Vec::<TextVertex>::new();
        let mut rect_instances = Vec::<StyledRectInstance>::new();

        let content = self.content_panel_rect();
        let file_pane = self.file_pane_rect();
        let diff_pane = self.diff_pane_rect();

        // ── Content background ───────────────────────────────
        push_styled_rect(
            &mut rect_instances,
            content,
            theme::palette().content_top,
            theme::palette().content_bottom,
            [0.0; 4],
            [0.0; 4],
            0.0,
            1.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        // ── Pane focus borders ───────────────────────────────
        let files_focused = self.focus_pane == FocusPane::Files;
        let diff_focused = self.focus_pane == FocusPane::Diff;

        // File pane top border (focus indicator) — teal accent
        if files_focused {
            push_styled_rect(
                &mut rect_instances,
                [file_pane[0], file_pane[1], file_pane[2], self.ui(2.0)],
                theme::palette().accent_blue,
                theme::palette().accent_blue,
                [0.0; 4],
                [0.0; 4],
                0.0,
                0.0,
                0.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );
        }

        // Diff pane top border (focus indicator) — teal accent
        if diff_focused {
            push_styled_rect(
                &mut rect_instances,
                [diff_pane[0], diff_pane[1], diff_pane[2], self.ui(2.0)],
                theme::palette().accent_blue,
                theme::palette().accent_blue,
                [0.0; 4],
                [0.0; 4],
                0.0,
                0.0,
                0.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );
        }

        // ── Vertical divider between panes ───────────────────
        let divider_x = file_pane[0] + file_pane[2];
        push_styled_rect(
            &mut rect_instances,
            [divider_x, content[1], self.ui(1.0), content[3]],
            theme::palette().divider,
            theme::palette().divider,
            [0.0; 4],
            [0.0; 4],
            0.0,
            0.0,
            0.0,
            0.0,
            [0.0, 0.0],
            0.0,
        );

        // ── Top chrome (titlebar, toolbar — rendered before panes) ─
        self.build_titlebar_geometry(&mut text_vertices, &mut rect_instances)?;
        self.build_toolbar_geometry(&mut text_vertices, &mut rect_instances)?;

        // ── File pane (left) ─────────────────────────────────
        {
            let pane = file_pane;
            let scroll = self.file_scroll_y;
            let pane_top = pane[1];
            let pane_bottom = pane[1] + pane[3];

            let visible_indices: Vec<usize> = self
                .file_visual_lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    let screen_y = line.y_top - scroll;
                    screen_y + self.line_height > pane_top && screen_y < pane_bottom
                })
                .map(|(idx, _)| idx)
                .collect();

            let selected_line = self.selected_file_line_index();

            for idx in visible_indices {
                self.ensure_file_line_shaped(idx);

                let (y_top, _line_index, line_style, glyphs) = {
                    let line = &self.file_visual_lines[idx];
                    (line.y_top, line.line_index, line.style, line.glyphs.clone())
                };

                let screen_y = y_top - scroll;

                // Selected row highlight
                if Some(idx) == selected_line {
                    push_styled_rect(
                        &mut rect_instances,
                        [
                            pane[0] + self.ui(4.0),
                            screen_y,
                            (pane[2] - self.ui(8.0)).max(1.0),
                            self.line_height,
                        ],
                        theme::palette().row_selected,
                        theme::palette().row_selected_bottom,
                        theme::palette().row_selected_border,
                        [0.0; 4],
                        self.ui(5.0),
                        1.0,
                        1.0,
                        0.0,
                        [0.0, 0.0],
                        0.0,
                    );
                    // Left accent bar
                    push_styled_rect(
                        &mut rect_instances,
                        [
                            pane[0] + self.ui(4.0),
                            screen_y + self.ui(2.0),
                            self.ui(3.0),
                            self.line_height - self.ui(4.0),
                        ],
                        theme::palette().selection_accent_bar,
                        theme::palette().selection_accent_bar,
                        [0.0; 4],
                        [0.0; 4],
                        self.ui(1.5),
                        0.5,
                        0.0,
                        0.0,
                        [0.0, 0.0],
                        0.0,
                    );
                }

                // Section header backgrounds
                if line_style.has_background() {
                    let (bg_top, bg_bottom, bg_border) = line_style.background_colors();
                    push_styled_rect(
                        &mut rect_instances,
                        [
                            pane[0] + self.ui(2.0),
                            screen_y,
                            (pane[2] - self.ui(4.0)).max(1.0),
                            self.line_height,
                        ],
                        bg_top,
                        bg_bottom,
                        bg_border,
                        [0.0; 4],
                        self.ui(3.0),
                        1.0,
                        1.0,
                        0.0,
                        [0.0, 0.0],
                        0.0,
                    );
                }

                // Render glyphs (clipped to pane)
                self.emit_glyphs_clipped(
                    &glyphs,
                    scroll,
                    pane[0],
                    pane[0] + pane[2],
                    pane_top,
                    pane_bottom,
                    &mut text_vertices,
                )?;
            }
        }

        // ── Diff pane (right) ────────────────────────────────
        {
            let pane = diff_pane;
            let scroll = self.diff_scroll_y;
            let pane_top = pane[1];
            let pane_bottom = pane[1] + pane[3];
            let ln_width = self.diff_line_number_width();

            // Line number gutter background
            push_styled_rect(
                &mut rect_instances,
                [pane[0], pane[1], ln_width, pane[3]],
                theme::palette().gutter_top,
                theme::palette().gutter_bottom,
                [0.0; 4],
                [0.0; 4],
                0.0,
                1.0,
                0.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            // Gutter right border
            push_styled_rect(
                &mut rect_instances,
                [pane[0] + ln_width, pane[1], 1.0, pane[3]],
                theme::palette().divider,
                theme::palette().divider,
                [0.0; 4],
                [0.0; 4],
                0.0,
                0.0,
                0.0,
                0.0,
                [0.0, 0.0],
                0.0,
            );

            let visible_indices: Vec<usize> = self
                .diff_visual_lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    let screen_y = line.y_top - scroll;
                    screen_y + self.line_height > pane_top && screen_y < pane_bottom
                })
                .map(|(idx, _)| idx)
                .collect();

            for idx in visible_indices {
                self.ensure_diff_line_shaped(idx);

                let (y_top, line_index, line_style, glyphs) = {
                    let line = &self.diff_visual_lines[idx];
                    (line.y_top, line.line_index, line.style, line.glyphs.clone())
                };

                let screen_y = y_top - scroll;

                // Line background tints for diff lines
                if line_style.has_background() {
                    let (bg_top, bg_bottom, bg_border) = line_style.background_colors();
                    let is_header = matches!(line_style, LineStyle::DiffFileHeader);
                    push_styled_rect(
                        &mut rect_instances,
                        [
                            pane[0] + ln_width + self.ui(2.0),
                            screen_y,
                            (pane[2] - ln_width - self.ui(4.0)).max(1.0),
                            self.line_height,
                        ],
                        bg_top,
                        bg_bottom,
                        if is_header { bg_border } else { [0.0; 4] },
                        [0.0; 4],
                        if is_header { self.ui(3.0) } else { self.ui(1.0) },
                        1.0,
                        if is_header || bg_border[3] > 0.0 {
                            1.0
                        } else {
                            0.0
                        },
                        0.0,
                        [0.0, 0.0],
                        0.0,
                    );
                }

                // Line numbers
                if let Some(ln) = self.diff_doc.line_number(line_index) {
                    let old_str = ln
                        .old
                        .map(|n| format!("{:>4}", n))
                        .unwrap_or_else(|| "    ".to_string());
                    let new_str = ln
                        .new
                        .map(|n| format!("{:>4}", n))
                        .unwrap_or_else(|| "    ".to_string());
                    let ln_text = format!("{} {} ", old_str, new_str);

                    let baseline = screen_y + self.ascent;
                    let mut ln_x = pane[0] + self.ui(4.0);
                    let ln_color = theme::palette().text_muted;
                    self.append_text_run(
                        &mut text_vertices,
                        &mut ln_x,
                        baseline,
                        &ln_text,
                        ln_color,
                    )?;
                }

                // Render glyphs (clipped to diff pane)
                self.emit_glyphs_clipped(
                    &glyphs,
                    scroll,
                    pane[0],
                    pane[0] + pane[2],
                    pane_top,
                    pane_bottom,
                    &mut text_vertices,
                )?;
            }
        }

        // ── Bottom chrome + modals (rendered AFTER panes so they overlay) ─
        self.build_status_geometry(&mut text_vertices, &mut rect_instances)?;
        self.build_modal_overlay_geometry(&mut text_vertices, &mut rect_instances)?;
        self.build_tooltip_geometry(&mut text_vertices, &mut rect_instances)?;

        self.text_vbuf = create_vertex_buffer(&self.device, "text_vertices", &text_vertices);
        self.text_vcount = text_vertices.len() as u32;

        self.rect_instance_vbuf =
            create_vertex_buffer(&self.device, "styled_rect_instances", &rect_instances);
        self.rect_instance_count = rect_instances.len() as u32;

        self.geometry_dirty = false;
        Ok(())
    }

    /// Emit glyph vertices clipped to a pane region.
    fn emit_glyphs_clipped(
        &mut self,
        glyphs: &[ShapedGlyph],
        scroll_y: f32,
        clip_left: f32,
        clip_right: f32,
        clip_top: f32,
        clip_bottom: f32,
        text_vertices: &mut Vec<TextVertex>,
    ) -> anyhow::Result<()> {
        for g in glyphs {
            let uv = match self.ensure_glyph(g.glyph_id)? {
                Some(uv) => uv,
                None => continue,
            };
            if uv.w == 0 || uv.h == 0 {
                continue;
            }

            let x0 = (g.x + uv.bearing_x).round();
            let y0 = (g.y + uv.bearing_y - scroll_y).round();
            let x1 = x0 + uv.w as f32;
            let y1 = y0 + uv.h as f32;

            // Skip glyphs fully outside the clip region
            if y1 < clip_top || y0 > clip_bottom || x1 < clip_left || x0 > clip_right {
                continue;
            }

            let color = g.color;
            text_vertices.push(TextVertex { pos: [x0, y0], uv: [uv.u0, uv.v0], color });
            text_vertices.push(TextVertex { pos: [x1, y0], uv: [uv.u1, uv.v0], color });
            text_vertices.push(TextVertex { pos: [x0, y1], uv: [uv.u0, uv.v1], color });
            text_vertices.push(TextVertex { pos: [x0, y1], uv: [uv.u0, uv.v1], color });
            text_vertices.push(TextVertex { pos: [x1, y0], uv: [uv.u1, uv.v0], color });
            text_vertices.push(TextVertex { pos: [x1, y1], uv: [uv.u1, uv.v1], color });
        }
        Ok(())
    }

    fn clamp_scroll(&mut self) {
        let file_pane = self.file_pane_rect();
        let diff_pane = self.diff_pane_rect();

        let file_max = (self.file_content_height - file_pane[3]).max(0.0);
        self.file_scroll_y = self.file_scroll_y.clamp(0.0, file_max);

        let diff_max = (self.diff_content_height - diff_pane[3]).max(0.0);
        self.diff_scroll_y = self.diff_scroll_y.clamp(0.0, diff_max);
    }

    fn file_line_at_y(&self, doc_y: f32) -> Option<usize> {
        self.file_visual_lines
            .iter()
            .enumerate()
            .find(|(_, line)| doc_y >= line.y_top && doc_y < line.y_top + self.line_height)
            .map(|(idx, _)| idx)
    }

    fn try_select_file_from_mouse(&mut self, pos: PhysicalPosition<f64>) -> anyhow::Result<bool> {
        let mx = pos.x as f32;
        let file_pane = self.file_pane_rect();

        // Only handle clicks in the file pane
        if mx < file_pane[0] || mx > file_pane[0] + file_pane[2] {
            // Click in diff pane — switch focus there
            let diff_pane = self.diff_pane_rect();
            if mx >= diff_pane[0] && mx <= diff_pane[0] + diff_pane[2] {
                self.focus_pane = FocusPane::Diff;
                self.geometry_dirty = true;
            }
            return Ok(false);
        }

        self.focus_pane = FocusPane::Files;
        let doc_y = pos.y as f32 + self.file_scroll_y;
        let Some(line_idx) = self.file_line_at_y(doc_y) else {
            return Ok(false);
        };

        let Some(target) = self.file_line_to_index.get(line_idx).and_then(|idx| *idx) else {
            return Ok(false);
        };
        if target == self.git.selected_index() {
            return Ok(false);
        }

        self.git.select_file_index(target)?;
        self.refresh_document_from_git()?;
        self.set_selection_status();
        Ok(true)
    }

    fn on_resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.configure_surface();
        self.update_uniform_screen();
        self.refresh_ui_scale();
    }

    fn apply_zoom(&mut self, delta: f32) {
        self.zoom_level = (self.zoom_level + delta).clamp(0.50, 2.50);
        self.refresh_ui_scale();
    }

    fn adjust_pane_ratio(&mut self, delta: f32) {
        self.file_pane_ratio = (self.file_pane_ratio + delta).clamp(0.10, 0.70);
        self.layout_dirty = true;
        self.geometry_dirty = true;
    }

    fn is_over_divider(&self, x: f32) -> bool {
        let file_pane = self.file_pane_rect();
        let divider_x = file_pane[0] + file_pane[2];
        let grab_zone = self.ui(5.0);
        (x - divider_x).abs() <= grab_zone
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta, modifiers_ctrl: bool) {
        let dy = match delta {
            MouseScrollDelta::LineDelta(_, y) => -y * self.line_height * 3.0,
            MouseScrollDelta::PixelDelta(p) => -(p.y as f32),
        };

        // Ctrl+scroll = zoom
        if modifiers_ctrl {
            let zoom_delta = if dy < 0.0 { 0.10 } else { -0.10 };
            self.apply_zoom(zoom_delta);
            return;
        }

        // Determine which pane the mouse is over
        let mx = self.mouse_pos.x as f32;
        let file_pane = self.file_pane_rect();
        let diff_pane = self.diff_pane_rect();

        if mx >= diff_pane[0] && mx <= diff_pane[0] + diff_pane[2] {
            self.diff_scroll_y += dy;
        } else if mx >= file_pane[0] && mx <= file_pane[0] + file_pane[2] {
            self.file_scroll_y += dy;
        } else {
            // Default to focused pane
            match self.focus_pane {
                FocusPane::Files => self.file_scroll_y += dy,
                FocusPane::Diff => self.diff_scroll_y += dy,
            }
        }
        self.clamp_scroll();
        self.geometry_dirty = true;
    }

    fn handle_key(&mut self, key: &Key) -> anyhow::Result<bool> {
        match key {
            Key::Named(NamedKey::Tab) => {
                // Toggle focus between panes
                self.focus_pane = match self.focus_pane {
                    FocusPane::Files => FocusPane::Diff,
                    FocusPane::Diff => FocusPane::Files,
                };
                self.geometry_dirty = true;
                Ok(true)
            }
            Key::Named(NamedKey::ArrowUp) => {
                match self.focus_pane {
                    FocusPane::Files => self.move_selection_and_refresh(-1),
                    FocusPane::Diff => {
                        self.diff_scroll_y -= self.line_height * 3.0;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                }
            }
            Key::Named(NamedKey::ArrowDown) => {
                match self.focus_pane {
                    FocusPane::Files => self.move_selection_and_refresh(1),
                    FocusPane::Diff => {
                        self.diff_scroll_y += self.line_height * 3.0;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                }
            }
            Key::Named(NamedKey::PageUp) => {
                match self.focus_pane {
                    FocusPane::Files => {
                        self.file_scroll_y -= self.file_pane_rect()[3] * 0.85;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                    FocusPane::Diff => {
                        self.diff_scroll_y -= self.diff_pane_rect()[3] * 0.85;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                }
            }
            Key::Named(NamedKey::PageDown) => {
                match self.focus_pane {
                    FocusPane::Files => {
                        self.file_scroll_y += self.file_pane_rect()[3] * 0.85;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                    FocusPane::Diff => {
                        self.diff_scroll_y += self.diff_pane_rect()[3] * 0.85;
                        self.clamp_scroll();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                }
            }
            Key::Character(ch) => {
                let raw = ch.as_ref();
                if raw == "P" {
                    return self.handle_toolbar_action(ToolbarAction::Push);
                }
                if raw == "U" {
                    return self.handle_toolbar_action(ToolbarAction::UnstageAll);
                }
                if raw == "A" {
                    return self.handle_toolbar_action(ToolbarAction::StageAll);
                }
                if raw == "X" {
                    return self.handle_toolbar_action(ToolbarAction::Discard);
                }
                if raw == "B" {
                    return self.handle_toolbar_action(ToolbarAction::BranchSwitch);
                }

                let c = raw.to_ascii_lowercase();
                match c.as_str() {
                    "h" => {
                        self.focus_pane = FocusPane::Files;
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                    "l" => {
                        self.focus_pane = FocusPane::Diff;
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                    "o" => self.handle_toolbar_action(ToolbarAction::RepoSwitch),
                    "b" => self.handle_toolbar_action(ToolbarAction::Browse),
                    "j" => match self.focus_pane {
                        FocusPane::Files => self.move_selection_and_refresh(1),
                        FocusPane::Diff => {
                            self.diff_scroll_y += self.line_height * 3.0;
                            self.clamp_scroll();
                            self.geometry_dirty = true;
                            Ok(true)
                        }
                    },
                    "k" => match self.focus_pane {
                        FocusPane::Files => self.move_selection_and_refresh(-1),
                        FocusPane::Diff => {
                            self.diff_scroll_y -= self.line_height * 3.0;
                            self.clamp_scroll();
                            self.geometry_dirty = true;
                            Ok(true)
                        }
                    },
                    "d" => {
                        Ok(self.execute_action("Diff backend toggled", |state| {
                            let backend = state.git.toggle_diff_backend()?;
                            state.refresh_document_from_git()?;
                            state.set_status(
                                StatusKind::Success,
                                format!("Diff backend: {}", backend.label()),
                            );
                            Ok(())
                        }))
                    }
                    "," => {
                        self.prompt_settings();
                        self.geometry_dirty = true;
                        Ok(true)
                    }
                    "r" => self.handle_toolbar_action(ToolbarAction::Refresh),
                    "s" => self.handle_toolbar_action(ToolbarAction::Stage),
                    "u" => self.handle_toolbar_action(ToolbarAction::Unstage),
                    "c" => self.handle_toolbar_action(ToolbarAction::Commit),
                    "a" => self.handle_toolbar_action(ToolbarAction::StageAll),
                    "f" => self.handle_toolbar_action(ToolbarAction::Fetch),
                    "p" => self.handle_toolbar_action(ToolbarAction::Pull),
                    "x" => self.handle_toolbar_action(ToolbarAction::Discard),
                    "=" | "+" => {
                        self.apply_zoom(0.10);
                        Ok(true)
                    }
                    "-" => {
                        self.apply_zoom(-0.10);
                        Ok(true)
                    }
                    "0" => {
                        self.zoom_level = 1.0;
                        self.refresh_ui_scale();
                        Ok(true)
                    }
                    "[" => {
                        self.adjust_pane_ratio(-0.05);
                        Ok(true)
                    }
                    "]" => {
                        self.adjust_pane_ratio(0.05);
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
                        load: wgpu::LoadOp::Clear(theme::palette().bg),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if self.rect_instance_count > 0 {
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bg, &[]);
                pass.set_vertex_buffer(0, self.rect_unit_vbuf.slice(..));
                pass.set_vertex_buffer(1, self.rect_instance_vbuf.slice(..));
                pass.draw(0..6, 0..self.rect_instance_count);
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

pub struct App {
    state: Option<State>,
    git: Option<GitModel>,
    modifiers: Modifiers,
}

impl App {
    fn new(git: GitModel) -> Self {
        Self {
            state: None,
            git: Some(git),
            modifiers: Modifiers::default(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Base attributes shared across platforms. The window itself is
        // opaque — we paint every pixel ourselves, and OS-level alpha
        // bleeding to the desktop is jarring under modals/scrims.
        let mut attrs = Window::default_attributes().with_title("wgit");

        // macOS: native frame with the title bar made transparent and
        // hidden, content extending into the title bar. macOS draws the
        // real traffic lights; our gradient paints behind them.
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs
                .with_titlebar_transparent(true)
                .with_title_hidden(true)
                .with_fullsize_content_view(true);
        }

        // Other platforms: keep the borderless, custom-drawn chrome.
        #[cfg(not(target_os = "macos"))]
        {
            attrs = attrs.with_decorations(false);
        }

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
            WindowEvent::ScaleFactorChanged { .. } => {
                st.refresh_ui_scale();
                needs_redraw = true;
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.modifiers = mods;
            }
            WindowEvent::CursorMoved { position, .. } => {
                st.mouse_pos = position;

                // Handle divider drag
                if st.divider_dragging {
                    let content = st.content_panel_rect();
                    if content[2] > 0.0 {
                        let ratio = (position.x as f32 - content[0]) / content[2];
                        st.file_pane_ratio = ratio.clamp(0.10, 0.70);
                        st.layout_dirty = true;
                        st.geometry_dirty = true;
                        needs_redraw = true;
                    }
                }

                // Update cursor icon for divider hover
                if st.is_over_divider(position.x as f32) || st.divider_dragging {
                    st.window.set_cursor(CursorIcon::EwResize);
                } else {
                    st.window.set_cursor(CursorIcon::Default);
                }

                let hovered = st.toolbar_action_at(position);
                if hovered != st.hover_toolbar_action {
                    let was_visible = st.tooltip_drawn;
                    st.hover_toolbar_action = hovered;
                    st.hover_started_at = hovered.map(|_| Instant::now());
                    st.tooltip_drawn = false;
                    if was_visible {
                        st.geometry_dirty = true;
                        needs_redraw = true;
                    }
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if st.hover_toolbar_action.is_some() {
                    let was_visible = st.tooltip_drawn;
                    st.hover_toolbar_action = None;
                    st.hover_started_at = None;
                    st.tooltip_drawn = false;
                    if was_visible {
                        st.geometry_dirty = true;
                        needs_redraw = true;
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state == ElementState::Released {
                    st.divider_dragging = false;
                }
                if state == ElementState::Pressed {
                    // Check divider drag before other click handlers
                    if st.is_over_divider(st.mouse_pos.x as f32) {
                        st.divider_dragging = true;
                    } else if let Some(action) = st.window_control_action_at(st.mouse_pos) {
                        match action {
                            WindowControlAction::Close => {
                                event_loop.exit();
                                return;
                            }
                            WindowControlAction::Minimize => st.window.set_minimized(true),
                            WindowControlAction::Zoom => {
                                let is_max = st.window.is_maximized();
                                st.window.set_maximized(!is_max);
                            }
                        }
                        needs_redraw = true;
                    } else if st.is_in_titlebar_drag_region(st.mouse_pos) {
                        if let Err(err) = st.window.drag_window() {
                            st.set_status(StatusKind::Error, format!("Drag window failed: {err}"));
                        }
                    } else if st.input_mode != InputMode::Normal {
                        needs_redraw = true;
                    } else if let Some(action) = st.toolbar_action_at(st.mouse_pos) {
                        if matches!(action, ToolbarAction::Quit) {
                            event_loop.exit();
                            return;
                        }

                        match st.handle_toolbar_action(action) {
                            Ok(changed) => needs_redraw |= changed,
                            Err(err) => {
                                st.set_status(
                                    StatusKind::Error,
                                    format!("Toolbar action failed: {err}"),
                                );
                                needs_redraw = true;
                            }
                        }
                    } else {
                        match st.try_select_file_from_mouse(st.mouse_pos) {
                            Ok(changed) => needs_redraw |= changed,
                            Err(err) => {
                                st.set_status(
                                    StatusKind::Error,
                                    format!("File selection failed: {err}"),
                                );
                                needs_redraw = true;
                            }
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let ctrl = self.modifiers.state().control_key();
                st.on_wheel(delta, ctrl);
                needs_redraw = true;
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    let handled = if st.input_mode != InputMode::Normal {
                        match st.input_mode {
                            InputMode::CommitSummary | InputMode::CommitBody => {
                                st.handle_commit_input(&event.logical_key)
                            }
                            InputMode::RepoPicker => {
                                st.handle_repo_picker_input(&event.logical_key)
                            }
                            InputMode::DiscardConfirm => {
                                st.handle_discard_confirm_input(&event.logical_key)
                            }
                            InputMode::BranchSwitcher => {
                                st.handle_branch_switcher_input(&event.logical_key)
                            }
                            InputMode::Settings => {
                                st.handle_settings_input(&event.logical_key)
                            }
                            InputMode::Normal => Ok(false),
                        }
                    } else {
                        if let Key::Character(ch) = &event.logical_key {
                            if ch.eq_ignore_ascii_case("q") {
                                event_loop.exit();
                                return;
                            }
                        }

                        match st.handle_key(&event.logical_key) {
                            Ok(changed) => needs_redraw |= changed,
                            Err(err) => {
                                st.set_status(
                                    StatusKind::Error,
                                    format!("Key handling failed: {err}"),
                                );
                                needs_redraw = true;
                            }
                        }
                        Ok(false)
                    };

                    match handled {
                        Ok(changed) => needs_redraw |= changed,
                        Err(err) => {
                            let label = match st.input_mode {
                                InputMode::CommitSummary | InputMode::CommitBody => {
                                    "Commit entry failed"
                                }
                                InputMode::RepoPicker => "Repo switch failed",
                                InputMode::DiscardConfirm => "Discard failed",
                                InputMode::BranchSwitcher => "Branch switch failed",
                                InputMode::Settings => "Settings failed",
                                InputMode::Normal => "Input handling failed",
                            };
                            st.set_status(StatusKind::Error, format!("{label}: {err}"));
                            needs_redraw = true;
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(err) = st.render() {
                    st.set_status(StatusKind::Error, format!("Render failed: {err}"));
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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(st) = self.state.as_mut() else {
            return;
        };
        let Some(started) = st.hover_started_at else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        let elapsed = started.elapsed();
        if elapsed < TOOLTIP_DELAY {
            event_loop
                .set_control_flow(ControlFlow::WaitUntil(started + TOOLTIP_DELAY));
        } else if !st.tooltip_drawn {
            st.geometry_dirty = true;
            st.window.request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}

pub fn run(git: GitModel) -> anyhow::Result<()> {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(git);
    event_loop.run_app(&mut app).expect("run app");
    Ok(())
}

fn compact_status_message(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = normalized.chars().count();
    if count <= max_chars {
        return normalized;
    }

    if max_chars <= 3 {
        return normalized.chars().take(max_chars).collect();
    }

    let mut out = String::with_capacity(max_chars);
    for ch in normalized.chars().take(max_chars - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn build_grouped_file_maps(
    doc: &Document,
    meta: &GroupedGitViewMeta,
) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut file_line_to_index = vec![None; doc.line_count()];
    let mut file_index_to_line = Vec::with_capacity(meta.files_count);

    for section in &meta.sections {
        for offset in 0..section.item_count {
            let line_idx = section.start_line + 1 + offset;
            if line_idx < file_line_to_index.len() {
                let file_index = file_index_to_line.len();
                file_line_to_index[line_idx] = Some(file_index);
                file_index_to_line.push(line_idx);
            }
        }
    }

    (file_line_to_index, file_index_to_line)
}

fn compute_ui_scale(scale_factor: f64, size: PhysicalSize<u32>) -> f32 {
    let scale_factor = scale_factor as f32;
    let density_scale = scale_factor.clamp(1.0, 2.0);
    let logical_w = size.width as f32 / scale_factor.max(1.0);
    let logical_h = size.height as f32 / scale_factor.max(1.0);
    let responsive_boost = if logical_w < 1200.0 || logical_h < 800.0 {
        0.95
    } else if logical_w > 1800.0 || logical_h > 1200.0 {
        1.08
    } else {
        1.0
    };

    (density_scale * responsive_boost).clamp(1.0, 2.25)
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

#[cfg(test)]
mod tests {
    use super::{compact_status_message, compute_ui_scale};
    use winit::dpi::PhysicalSize;

    #[test]
    fn compacts_whitespace_and_newlines() {
        let message = compact_status_message("stage\nselected\tfile", 64);
        assert_eq!(message, "stage selected file");
    }

    #[test]
    fn truncates_with_ascii_ellipsis() {
        let message = compact_status_message("abcdefghij", 5);
        assert_eq!(message, "ab...");
    }

    #[test]
    fn boosts_ui_scale_for_retina_density() {
        let scale = compute_ui_scale(2.0, PhysicalSize::new(3024, 1964));
        assert!(scale >= 1.9);
    }

    #[test]
    fn keeps_standard_density_near_baseline() {
        let scale = compute_ui_scale(1.0, PhysicalSize::new(1440, 900));
        assert!((scale - 1.0).abs() < f32::EPSILON);
    }
}
