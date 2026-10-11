//! Pixel-level comparison of two screenshots, to tell a still screen from one
//! that is loading or animating (a spinner carries no text for OCR to notice).

use anyhow::{Context, Result};

/// Grid the 1280x720 frame is averaged down to: 8x8-pixel cells.
const GW: usize = 160;
const GH: usize = 90;

/// Grayscale cell averages of a JPEG screenshot.
pub fn thumb(jpg: &[u8]) -> Result<Vec<u8>> {
    let mut dec = jpeg_decoder::Decoder::new(jpg);
    let px = dec.decode().context("decode screenshot")?;
    let info = dec.info().context("screenshot has no header")?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = px.len() / (w * h).max(1);
    let mut sum = vec![0u32; GW * GH];
    let mut n = vec![0u32; GW * GH];
    for y in 0..h {
        let gy = y * GH / h;
        for x in 0..w {
            let i = (y * w + x) * ch;
            let v = if ch >= 3 { (px[i] as u32 * 3 + px[i + 1] as u32 * 6 + px[i + 2] as u32) / 10 } else { px[i] as u32 };
            let g = gy * GW + x * GW / w;
            sum[g] += v;
            n[g] += 1;
        }
    }
    Ok(sum.iter().zip(&n).map(|(s, n)| (s / (*n).max(1)) as u8).collect())
}

/// Cells (of 14400) whose brightness moved by more than 60 between two thumbs.
/// Measured on a console: the HOME focus highlight pulses 0-5 cells, a loading
/// page 1700+, a moved selection frame about a hundred.
pub fn changed(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x.abs_diff(**y) > 60).count()
}

/// At most this many changed cells counts as a still screen.
pub const STILL: usize = 8;

pub fn still(a: &[u8], b: &[u8]) -> bool {
    changed(a, b) <= STILL
}
