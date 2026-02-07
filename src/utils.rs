use std::convert;

use ab_glyph::{Font, FontRef, Glyph, point};

pub struct RasterGlyph {
    pub w: u32,
    pub h: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance_x: f32,
    pub alpha: Vec<u8>, // w * h bytes, row-major
}

pub fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}

pub fn read_font_bytes(path: &str) -> Result<Vec<u8>, std::io::Error> {
    std::fs::read(path)
}

pub fn load_font() -> Result<FontRef<'static>, ab_glyph::InvalidFont> {
    FontRef::try_from_slice(include_bytes!("../data/fonts/Terminus.ttf"))
}

pub fn get_glyph(glyph: char, font: &FontRef) -> Glyph {
    font.glyph_id(glyph)
        .with_scale_and_position(24.0, point(100.0, 0.0))
}

pub fn draw_file() {
    let file = read_file("../main.cpp").unwrap();
    let font = load_font().unwrap();
    for c in file.chars() {
        let glyph = get_glyph(c, &font);
        draw_glyph(glyph, &font);
    }
}

pub fn draw_glyph(g: Glyph, font: &FontRef) -> Option<RasterGlyph> {
    let Some(outline) = font.outline_glyph(g.clone()) else {
        return None;
    };
    let bounds = outline.px_bounds();
    let w = bounds.width() as u32;
    let h = bounds.height() as u32;
    let mut alpha = vec![0u8; (w * h) as usize];

    outline.draw(|x, y, cov| {
        let idx = (y as u32 * w + x as u32) as usize;
        alpha[idx] = (cov.clamp(0.0, 1.0) * 255.0).round() as u8;
    });

    Some(RasterGlyph {
        w,
        h,
        bearing_x: bounds.min.x,
        bearing_y: bounds.min.y,
        advance_x: font.h_advance_unscaled(g.id) * g.clone().scale.x,
        alpha,
    })
}
