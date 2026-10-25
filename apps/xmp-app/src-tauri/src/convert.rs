//! Pixel format conversion between the web canvas (RGBA) and the codec (I420, BT.601 limited range).

use xmp_codec::video::Yuv420;

pub fn rgba_to_i420(rgba: &[u8], w: usize, h: usize) -> Yuv420 {
    let mut f = Yuv420::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, b) = (rgba[i] as i32, rgba[i + 1] as i32, rgba[i + 2] as i32);
            f.y[y * w + x] = (((66 * r + 129 * g + 25 * b + 128) >> 8) + 16).clamp(0, 255) as u8;
        }
    }
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let i = ((y * 2 + dy) * w + x * 2 + dx) * 4;
                r += rgba[i] as i32;
                g += rgba[i + 1] as i32;
                b += rgba[i + 2] as i32;
            }
            let (r, g, b) = (r / 4, g / 4, b / 4);
            f.u[y * (w / 2) + x] = (((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
            f.v[y * (w / 2) + x] = (((112 * r - 94 * g - 18 * b + 128) >> 8) + 128).clamp(0, 255) as u8;
        }
    }
    f
}

/// Output: 8-byte header (`w`, `h` as u32 LE) followed by RGBA.
pub fn i420_to_rgba_frame(f: &Yuv420) -> Vec<u8> {
    let (w, h) = (f.width, f.height);
    let mut out = Vec::with_capacity(8 + w * h * 4);
    out.extend_from_slice(&(w as u32).to_le_bytes());
    out.extend_from_slice(&(h as u32).to_le_bytes());
    for y in 0..h {
        for x in 0..w {
            let c = f.y[y * w + x] as i32 - 16;
            let d = f.u[(y / 2) * (w / 2) + x / 2] as i32 - 128;
            let e = f.v[(y / 2) * (w / 2) + x / 2] as i32 - 128;
            let r = (298 * c + 409 * e + 128) >> 8;
            let g = (298 * c - 100 * d - 208 * e + 128) >> 8;
            let b = (298 * c + 516 * d + 128) >> 8;
            out.extend_from_slice(&[r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8, 255]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_colors_roundtrip() {
        for (r, g, b) in [(255u8, 0u8, 0u8), (0, 255, 0), (0, 0, 255), (255, 255, 255), (0, 0, 0), (120, 90, 200)] {
            let (w, h) = (16, 16);
            let rgba: Vec<u8> = (0..w * h).flat_map(|_| [r, g, b, 255]).collect();
            let out = i420_to_rgba_frame(&rgba_to_i420(&rgba, w, h));
            assert_eq!(&out[..8], &[16, 0, 0, 0, 16, 0, 0, 0]);
            for px in out[8..].chunks(4) {
                for (a, e) in px[..3].iter().zip([r, g, b]) {
                    assert!((*a as i32 - e as i32).abs() <= 4, "({r},{g},{b}) -> {px:?}");
                }
            }
        }
    }
}
