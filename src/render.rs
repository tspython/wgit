use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct TextVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct QuadVertex {
    pub unit: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct StyledRectInstance {
    pub rect: [f32; 4],
    pub fill_top: [f32; 4],
    pub fill_bottom: [f32; 4],
    pub stroke: [f32; 4],
    pub shadow: [f32; 4],
    pub radius_soft_border_blur: [f32; 4],
    pub shadow_offset_spread: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Uniforms {
    pub screen_w: f32,
    pub screen_h: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GlyphUV {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
    pub w: u32,
    pub h: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct GlyphKey {
    pub glyph_id: u16,
    pub px_q: u16,
}

pub type GlyphCache = HashMap<GlyphKey, GlyphUV>;

pub struct Atlas {
    pub tex: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub w: u32,
    pub h: u32,
    cursor_x: u32,
    cursor_y: u32,
    shelf_h: u32,
}

impl Atlas {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, w: u32, h: u32) -> Self {
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

    pub fn alloc(&mut self, gw: u32, gh: u32) -> Option<(u32, u32)> {
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

pub fn create_empty_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: false,
    })
}

pub fn create_vertex_buffer<T: Pod>(
    device: &wgpu::Device,
    label: &str,
    verts: &[T],
) -> wgpu::Buffer {
    if verts.is_empty() {
        return create_empty_buffer(device, label, std::mem::size_of::<T>() as u64);
    }
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytemuck::cast_slice(verts),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn push_styled_rect(
    out: &mut Vec<StyledRectInstance>,
    rect: [f32; 4],
    fill_top: [f32; 4],
    fill_bottom: [f32; 4],
    stroke: [f32; 4],
    shadow: [f32; 4],
    radius: f32,
    softness: f32,
    border: f32,
    shadow_blur: f32,
    shadow_offset: [f32; 2],
    shadow_spread: f32,
) {
    if rect[2] <= 0.0 || rect[3] <= 0.0 {
        return;
    }

    out.push(StyledRectInstance {
        rect,
        fill_top,
        fill_bottom,
        stroke,
        shadow,
        radius_soft_border_blur: [radius, softness, border, shadow_blur],
        shadow_offset_spread: [shadow_offset[0], shadow_offset[1], shadow_spread, 0.0],
    });
}
