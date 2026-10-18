//! The application icon: a white battery outline with a green charge bar on a dark rounded
//! tile. It is drawn from geometry snapped to whole pixels at each size, so every entry is
//! sharp. The build script embeds it in the executable; the tray writes it out for toasts.

const TILE: [u8; 3] = [0x2B, 0x2B, 0x2B];
const OUTLINE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const CHARGE: [u8; 3] = [0x22, 0xC5, 0x5E];

/// The sizes in the `.ico`, covering the shell's small, medium and large views at every
/// scale.
pub const SIZES: [u32; 9] = [16, 20, 24, 32, 40, 48, 64, 128, 256];

/// An axis-aligned rectangle in pixels, with rounded corners.
struct Shape {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
}

impl Shape {
    fn contains(&self, x: f32, y: f32) -> bool {
        if x < self.x0 || x >= self.x1 || y < self.y0 || y >= self.y1 {
            return false;
        }
        let cx = x.clamp(self.x0 + self.radius, self.x1 - self.radius);
        let cy = y.clamp(self.y0 + self.radius, self.y1 - self.radius);
        (x - cx).powi(2) + (y - cy).powi(2) <= self.radius.powi(2)
    }
}

/// Straight-alpha RGBA pixels of the icon at `size` × `size`.
pub fn rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let px = |f: f32| (s * f).round();
    let stroke = px(1.0 / 16.0).max(1.0);

    let tile = Shape {
        x0: 0.0,
        y0: 0.0,
        x1: s,
        y1: s,
        radius: s * 0.19,
    };
    // The battery body and its terminal nub, centred together.
    let body_w = px(0.62);
    let body_h = px(0.40);
    let nub_w = (stroke * 1.5).round().max(1.0);
    let nub_h = px(0.16).max(2.0);
    let x0 = ((s - body_w - nub_w) / 2.0).round();
    let y0 = ((s - body_h) / 2.0).round();
    let body = Shape {
        x0,
        y0,
        x1: x0 + body_w,
        y1: y0 + body_h,
        radius: stroke,
    };
    let hollow = Shape {
        x0: x0 + stroke,
        y0: y0 + stroke,
        x1: x0 + body_w - stroke,
        y1: y0 + body_h - stroke,
        radius: 0.0,
    };
    let ny0 = (y0 + (body_h - nub_h) / 2.0).round();
    let nub = Shape {
        x0: x0 + body_w,
        y0: ny0,
        x1: x0 + body_w + nub_w,
        y1: ny0 + nub_h,
        radius: 0.0,
    };
    // The charge bar fills about two thirds of the hollow, one stroke in from the outline.
    let gap = stroke;
    let inner_w = body_w - 2.0 * (stroke + gap);
    let charge = Shape {
        x0: x0 + stroke + gap,
        y0: y0 + stroke + gap,
        x1: x0 + stroke + gap + (inner_w * 0.68).round().max(1.0),
        y1: y0 + body_h - stroke - gap,
        radius: 0.0,
    };

    // 4 × 4 samples per pixel smooth the tile's rounded corners.
    const SAMPLES: u32 = 4;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..size {
        for x in 0..size {
            let mut rgb = [0.0f32; 3];
            let mut alpha = 0.0f32;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let fx = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let fy = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    if !tile.contains(fx, fy) {
                        continue;
                    }
                    let colour = if charge.contains(fx, fy) {
                        CHARGE
                    } else if (body.contains(fx, fy) && !hollow.contains(fx, fy))
                        || nub.contains(fx, fy)
                    {
                        OUTLINE
                    } else {
                        TILE
                    };
                    for (sum, c) in rgb.iter_mut().zip(colour) {
                        *sum += f32::from(c);
                    }
                    alpha += 1.0;
                }
            }
            let pixel = &mut out[((y * size + x) * 4) as usize..][..4];
            if alpha > 0.0 {
                for (p, sum) in pixel.iter_mut().zip(rgb) {
                    *p = (sum / alpha).round() as u8;
                }
            }
            pixel[3] = (alpha / (SAMPLES * SAMPLES) as f32 * 255.0).round() as u8;
        }
    }
    out
}

/// A `.ico` file holding the icon at every size in [`SIZES`], as 32-bit DIB entries.
pub fn ico() -> Vec<u8> {
    let images: Vec<Vec<u8>> = SIZES.iter().map(|&size| dib(size)).collect();
    let mut file = Vec::new();
    file.extend_from_slice(&[0, 0, 1, 0]);
    file.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (&size, image) in SIZES.iter().zip(&images) {
        // 256 is stored as 0 in the one-byte width and height fields.
        let edge = if size >= 256 { 0 } else { size as u8 };
        file.extend_from_slice(&[edge, edge, 0, 0]);
        file.extend_from_slice(&1u16.to_le_bytes());
        file.extend_from_slice(&32u16.to_le_bytes());
        file.extend_from_slice(&(image.len() as u32).to_le_bytes());
        file.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in images {
        file.extend_from_slice(&image);
    }
    file
}

/// One icon entry: a BITMAPINFOHEADER, bottom-up BGRA pixels, and an empty AND mask.
fn dib(size: u32) -> Vec<u8> {
    let pixels = rgba(size);
    let mask_row = size.div_ceil(32) * 4;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(size as i32).to_le_bytes());
    // The height counts the colour bitmap and the mask together.
    out.extend_from_slice(&(2 * size as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 24]);
    for y in (0..size).rev() {
        for x in 0..size {
            let p = &pixels[((y * size + x) * 4) as usize..][..4];
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}
