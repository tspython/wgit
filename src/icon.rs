//! Tiny SVG path icon renderer.
//!
//! Parses a small subset of SVG path syntax — `M`, `L`, `H`, `V`, `C`, `Z`
//! and their lowercase relative-form counterparts — flattens cubic
//! Béziers into line segments, and rasterizes the resulting filled
//! polygons to a single-channel R8 alpha bitmap with 4×4 supersampled
//! anti-aliasing using the even-odd fill rule.
//!
//! Strokes are not supported — the bundled icon set is fill-only,
//! so this is enough.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Pt {
    x: f32,
    y: f32,
}

#[derive(Clone, Debug, Default)]
pub struct PathData {
    /// Each subpath is a closed polygon (last point == first).
    pub subpaths: Vec<Vec<Pt>>,
}

#[derive(Clone, Copy, Debug)]
enum Token {
    Cmd(char),
    Num(f32),
}

/// Parse SVG path `d` attribute into flattened polygons.
pub fn parse_path(d: &str) -> PathData {
    let tokens = tokenize(d);
    build_paths(&tokens)
}

fn tokenize(d: &str) -> Vec<Token> {
    let bytes = d.as_bytes();
    let mut tokens: Vec<Token> = Vec::with_capacity(bytes.len() / 4);
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Whitespace / commas separate tokens.
        if b.is_ascii_whitespace() || b == b',' {
            i += 1;
            continue;
        }
        if b.is_ascii_alphabetic() {
            tokens.push(Token::Cmd(b as char));
            i += 1;
            continue;
        }
        // Number — sign, digits, optional fractional, optional exponent.
        let start = i;
        if b == b'-' || b == b'+' {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i > start {
            if let Ok(s) = std::str::from_utf8(&bytes[start..i]) {
                if let Ok(n) = s.parse::<f32>() {
                    tokens.push(Token::Num(n));
                    continue;
                }
            }
        }
        // Unknown byte — skip
        i += 1;
    }
    tokens
}

fn build_paths(tokens: &[Token]) -> PathData {
    let mut subpaths: Vec<Vec<Pt>> = Vec::new();
    let mut current: Vec<Pt> = Vec::new();
    let mut start = Pt::default();
    let mut pos = Pt::default();
    let mut last_cmd = ' ';
    let mut i: usize = 0;

    let read_num = |idx: &mut usize| -> Option<f32> {
        match tokens.get(*idx) {
            Some(Token::Num(n)) => {
                *idx += 1;
                Some(*n)
            }
            _ => None,
        }
    };

    while i < tokens.len() {
        let cmd = match tokens[i] {
            Token::Cmd(c) => {
                i += 1;
                c
            }
            // Implicit repetition of last command. After a moveto, repeats
            // become linetos per the SVG spec.
            Token::Num(_) => match last_cmd {
                'M' => 'L',
                'm' => 'l',
                c => c,
            },
        };

        match cmd {
            'M' | 'm' => {
                let Some(x) = read_num(&mut i) else { break };
                let Some(y) = read_num(&mut i) else { break };
                let p = if cmd == 'M' {
                    Pt { x, y }
                } else {
                    Pt {
                        x: pos.x + x,
                        y: pos.y + y,
                    }
                };
                if !current.is_empty() {
                    let f = current[0];
                    if *current.last().unwrap() != f {
                        current.push(f);
                    }
                    subpaths.push(std::mem::take(&mut current));
                }
                current.push(p);
                pos = p;
                start = p;
                last_cmd = cmd;
            }
            'L' | 'l' => {
                let Some(x) = read_num(&mut i) else { break };
                let Some(y) = read_num(&mut i) else { break };
                let p = if cmd == 'L' {
                    Pt { x, y }
                } else {
                    Pt {
                        x: pos.x + x,
                        y: pos.y + y,
                    }
                };
                current.push(p);
                pos = p;
                last_cmd = cmd;
            }
            'H' | 'h' => {
                let Some(x) = read_num(&mut i) else { break };
                let nx = if cmd == 'H' { x } else { pos.x + x };
                let p = Pt { x: nx, y: pos.y };
                current.push(p);
                pos = p;
                last_cmd = cmd;
            }
            'V' | 'v' => {
                let Some(y) = read_num(&mut i) else { break };
                let ny = if cmd == 'V' { y } else { pos.y + y };
                let p = Pt { x: pos.x, y: ny };
                current.push(p);
                pos = p;
                last_cmd = cmd;
            }
            'C' | 'c' => {
                let Some(x1) = read_num(&mut i) else { break };
                let Some(y1) = read_num(&mut i) else { break };
                let Some(x2) = read_num(&mut i) else { break };
                let Some(y2) = read_num(&mut i) else { break };
                let Some(x) = read_num(&mut i) else { break };
                let Some(y) = read_num(&mut i) else { break };
                let (c1, c2, p) = if cmd == 'C' {
                    (Pt { x: x1, y: y1 }, Pt { x: x2, y: y2 }, Pt { x, y })
                } else {
                    (
                        Pt {
                            x: pos.x + x1,
                            y: pos.y + y1,
                        },
                        Pt {
                            x: pos.x + x2,
                            y: pos.y + y2,
                        },
                        Pt {
                            x: pos.x + x,
                            y: pos.y + y,
                        },
                    )
                };
                flatten_cubic(pos, c1, c2, p, 0.25, &mut current);
                pos = p;
                last_cmd = cmd;
            }
            'Z' | 'z' => {
                if !current.is_empty() {
                    let f = current[0];
                    if *current.last().unwrap() != f {
                        current.push(f);
                    }
                    subpaths.push(std::mem::take(&mut current));
                }
                pos = start;
                last_cmd = cmd;
            }
            _ => {
                // Unsupported command (S, s, Q, q, T, t, A, a) — bail out.
                // The bundled icon set only uses M/L/H/V/C/Z.
                break;
            }
        }
    }
    if !current.is_empty() {
        let f = current[0];
        if *current.last().unwrap() != f {
            current.push(f);
        }
        subpaths.push(current);
    }
    PathData { subpaths }
}

/// Recursive de Casteljau subdivision until the curve is flat enough.
/// `tol` is the perpendicular-distance threshold relative to the chord
/// length; 0.25 viewBox-units gives smooth-looking 24×24 icons.
fn flatten_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, tol: f32, out: &mut Vec<Pt>) {
    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let chord_sq = dx * dx + dy * dy;

    // Distance from p1, p2 to the chord (unnormalized cross-product).
    let d1 = ((p1.y - p0.y) * dx - (p1.x - p0.x) * dy).abs();
    let d2 = ((p2.y - p0.y) * dx - (p2.x - p0.x) * dy).abs();
    let len = chord_sq.sqrt().max(1e-6);

    // Stop subdividing when the curve is close to its chord.
    if d1.max(d2) / len < tol || chord_sq < tol * tol * 4.0 {
        out.push(p3);
        return;
    }

    let mid = |a: Pt, b: Pt| Pt {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    };
    let q0 = mid(p0, p1);
    let q1 = mid(p1, p2);
    let q2 = mid(p2, p3);
    let r0 = mid(q0, q1);
    let r1 = mid(q1, q2);
    let s = mid(r0, r1);

    flatten_cubic(p0, q0, r0, s, tol, out);
    flatten_cubic(s, r1, q2, p3, tol, out);
}

/// Even-odd point-in-polygon test using the crossing-number method.
fn point_in_polygon(p: Pt, poly: &[Pt]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let a = poly[j];
        let b = poly[i];
        if (a.y > p.y) != (b.y > p.y) {
            let dy = b.y - a.y;
            if dy.abs() > 1e-12 {
                let t = (p.y - a.y) / dy;
                let x_cross = a.x + t * (b.x - a.x);
                if p.x < x_cross {
                    inside = !inside;
                }
            }
        }
        j = i;
    }
    inside
}

/// Rasterize a parsed path into an `pixel_w × pixel_h` R8 alpha bitmap.
/// The path is expected to be in `view_w × view_h` SVG coordinates.
/// Uses 4×4 supersampling and the even-odd fill rule (matches SVG
/// fill-rule "evenodd" exactly — the bundled icons rely on this to
/// punch holes via overlapping subpaths).
pub fn rasterize_filled(
    paths: &PathData,
    view_w: f32,
    view_h: f32,
    pixel_w: u32,
    pixel_h: u32,
) -> Vec<u8> {
    let scale_x = pixel_w as f32 / view_w;
    let scale_y = pixel_h as f32 / view_h;
    let n: u32 = 4;
    let inv_n = 1.0 / n as f32;
    let total = (n * n) as f32;

    let mut out = vec![0u8; (pixel_w * pixel_h) as usize];

    // Precompute per-subpath bounding boxes so we can skip cheaply.
    let bboxes: Vec<(f32, f32, f32, f32)> = paths
        .subpaths
        .iter()
        .map(|sub| {
            let mut x0 = f32::INFINITY;
            let mut y0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut y1 = f32::NEG_INFINITY;
            for p in sub {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            (x0, y0, x1, y1)
        })
        .collect();

    for py in 0..pixel_h {
        for px in 0..pixel_w {
            let mut hits: u32 = 0;
            for sy in 0..n {
                for sx in 0..n {
                    let svgx = (px as f32 + (sx as f32 + 0.5) * inv_n) / scale_x;
                    let svgy = (py as f32 + (sy as f32 + 0.5) * inv_n) / scale_y;
                    let probe = Pt { x: svgx, y: svgy };
                    let mut inside = false;
                    for (sub, bb) in paths.subpaths.iter().zip(bboxes.iter()) {
                        if probe.x < bb.0 || probe.x > bb.2 || probe.y < bb.1 || probe.y > bb.3 {
                            continue;
                        }
                        if point_in_polygon(probe, sub) {
                            inside = !inside;
                        }
                    }
                    if inside {
                        hits += 1;
                    }
                }
            }
            let alpha = (hits as f32 / total * 255.0).round() as u8;
            out[(py * pixel_w + px) as usize] = alpha;
        }
    }

    out
}

/// Extract the first `d="..."` attribute value from an SVG document.
/// The bundled icons have a single `<path>` per file, so the first
/// match is all we need.
pub fn extract_path_d(svg: &str) -> Option<&str> {
    let needle = "d=\"";
    let start = svg.find(needle)? + needle.len();
    let rest = &svg[start..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Extract the `viewBox` (min-x, min-y, width, height) from an SVG.
/// Falls back to (0, 0, 24, 24) — the most common viewBox — if the
/// attribute is missing or malformed.
pub fn extract_viewbox(svg: &str) -> (f32, f32, f32, f32) {
    if let Some(start) = svg.find("viewBox=\"") {
        let s = &svg[start + "viewBox=\"".len()..];
        if let Some(end) = s.find('"') {
            let parts: Vec<f32> = s[..end]
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse::<f32>().ok())
                .collect();
            if parts.len() == 4 {
                return (parts[0], parts[1], parts[2], parts[3]);
            }
        }
    }
    (0.0, 0.0, 24.0, 24.0)
}
