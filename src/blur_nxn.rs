//! Általánosított (tetszőleges páratlan kernelméretű) Gaussian Blur, a
//! `blur.rs`-ben kidolgozott szeparábilis + csempézett + kígyózó technika
//! általánosítása. Ezt KIZÁRÓLAG a méret-/kernelméret-sweep benchmark
//! (`src/bin/size_kernel_sweep.rs`) használja -- a `blur.rs` fix 3x3-as,
//! lépésről lépésre kidolgozott implementációi változatlanok maradtak.
//!
//! Részletes magyarázat: docs/blur_nxn.md

use image::{Rgb, RgbImage};

/// Legenerál egy `2*radius+1` hosszú, BINOMIÁLIS (Pascal-háromszög) 1D
/// kernelt, ami a Gauss-görbe egy jó, egész-értékű közelítése. `radius=1`
/// esetén pontosan a `blur.rs::KERNEL_1D` `[1,2,1]`-et adja vissza.
pub fn binomial_kernel_1d(radius: usize) -> Vec<f32> {
    let n = 2 * radius;
    let mut result = Vec::with_capacity(n + 1);
    let mut c = 1.0f64;
    for k in 0..=n {
        result.push(c as f32);
        c = c * (n - k) as f64 / (k + 1) as f64;
    }
    result
}

/// Direkt (naiv) 2D konvolúció, tetszőleges (szimmetrikus, szeparábilis) 1D
/// kernellel -- FÜGGETLEN referencia-implementáció a szeparábilis/csempézett
/// eredmények ellenőrzéséhez, minden kernelméretnél újra.
pub fn gaussian_blur_direct(input: &RgbImage, kernel_1d: &[f32]) -> RgbImage {
    let radius = (kernel_1d.len() / 2) as i64;
    let kernel_sum: f32 = kernel_1d.iter().sum();
    let norm = kernel_sum * kernel_sum;

    let (width, height) = input.dimensions();
    let mut output = RgbImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];
            for (ky, &wy) in kernel_1d.iter().enumerate() {
                let sy = (y as i64 + ky as i64 - radius).clamp(0, height as i64 - 1) as u32;
                for (kx, &wx) in kernel_1d.iter().enumerate() {
                    let sx = (x as i64 + kx as i64 - radius).clamp(0, width as i64 - 1) as u32;
                    let pixel = input.get_pixel(sx, sy);
                    let weight = wx * wy;
                    for c in 0..3usize {
                        sum[c] += pixel[c] as f32 * weight;
                    }
                }
            }
            let r = (sum[0] / norm).round() as u8;
            let g = (sum[1] / norm).round() as u8;
            let b = (sum[2] / norm).round() as u8;
            output.put_pixel(x, y, Rgb([r, g, b]));
        }
    }

    output
}

const TILE_SIZE: u32 = 16;

/// A `blur.rs::gaussian_blur_3x3_tiled_mt` általánosítása tetszőleges
/// páratlan hosszú `kernel_1d`-re. Ugyanaz a csempézett + kígyózó bejárású +
/// szálankénti diszjunkt szeletekre bontott felépítés, csak a "halo" mérete
/// most `radius` (nem fixen 1).
pub fn gaussian_blur_tiled_mt(input: &RgbImage, kernel_1d: &[f32], num_threads: usize) -> RgbImage {
    let (width, height) = input.dimensions();
    let mut output = RgbImage::new(width, height);

    let tiles_y = height.div_ceil(TILE_SIZE);
    let tile_rows_per_thread = tiles_y.div_ceil(num_threads as u32);
    let row_stride = width as usize * 3;
    let chunk_bytes = tile_rows_per_thread as usize * TILE_SIZE as usize * row_stride;

    let raw: &mut [u8] = &mut output;

    std::thread::scope(|scope| {
        for (chunk_index, chunk) in raw.chunks_mut(chunk_bytes).enumerate() {
            let tile_row_start = chunk_index as u32 * tile_rows_per_thread;
            let tile_row_end = (tile_row_start + tile_rows_per_thread).min(tiles_y);
            let row_offset = tile_row_start * TILE_SIZE;

            scope.spawn(move || {
                process_tile_row_range(
                    input,
                    chunk,
                    row_stride,
                    row_offset,
                    width,
                    tile_row_start,
                    tile_row_end,
                    kernel_1d,
                );
            });
        }
    });

    output
}

fn process_tile_row_range(
    input: &RgbImage,
    output: &mut [u8],
    row_stride: usize,
    row_offset: u32,
    width: u32,
    tile_row_start: u32,
    tile_row_end: u32,
    kernel_1d: &[f32],
) {
    let (_, height) = input.dimensions();
    let tiles_x = width.div_ceil(TILE_SIZE);

    for tile_row in tile_row_start..tile_row_end {
        let left_to_right = tile_row % 2 == 0;

        for i in 0..tiles_x {
            let tile_col = if left_to_right { i } else { tiles_x - 1 - i };

            let tile_x0 = tile_col * TILE_SIZE;
            let tile_y0 = tile_row * TILE_SIZE;
            let tile_w = TILE_SIZE.min(width - tile_x0);
            let tile_h = TILE_SIZE.min(height - tile_y0);

            process_tile(input, output, row_stride, row_offset, tile_x0, tile_y0, tile_w, tile_h, kernel_1d);
        }
    }
}

fn process_tile(
    input: &RgbImage,
    output: &mut [u8],
    row_stride: usize,
    row_offset: u32,
    tile_x0: u32,
    tile_y0: u32,
    tile_w: u32,
    tile_h: u32,
    kernel_1d: &[f32],
) {
    let (width, height) = input.dimensions();
    let radius = (kernel_1d.len() / 2) as i64;
    let kernel_sum: f32 = kernel_1d.iter().sum();
    let norm = kernel_sum * kernel_sum;

    let padded_h = tile_h + 2 * radius as u32;
    let mut intermediate = vec![0.0f32; (tile_w as usize) * (padded_h as usize) * 3];

    for ky in 0..padded_h {
        let y = (tile_y0 as i64 + ky as i64 - radius).clamp(0, height as i64 - 1) as u32;

        for kx in 0..tile_w {
            let x = tile_x0 + kx;
            let mut sum = [0.0f32; 3];

            for (dx, &weight) in kernel_1d.iter().enumerate() {
                let sample_x = (x as i64 + dx as i64 - radius).clamp(0, width as i64 - 1) as u32;
                let pixel = input.get_pixel(sample_x, y);
                for c in 0..3usize {
                    sum[c] += pixel[c] as f32 * weight;
                }
            }

            let base = ((ky * tile_w + kx) * 3) as usize;
            intermediate[base] = sum[0];
            intermediate[base + 1] = sum[1];
            intermediate[base + 2] = sum[2];
        }
    }

    for oy in 0..tile_h {
        let local_y = (tile_y0 + oy - row_offset) as usize;

        for ox in 0..tile_w {
            let mut sum = [0.0f32; 3];

            for (dy, &weight) in kernel_1d.iter().enumerate() {
                let iy = oy + dy as u32;
                let base = ((iy * tile_w + ox) * 3) as usize;
                sum[0] += intermediate[base] * weight;
                sum[1] += intermediate[base + 1] * weight;
                sum[2] += intermediate[base + 2] * weight;
            }

            let r = (sum[0] / norm).round() as u8;
            let g = (sum[1] / norm).round() as u8;
            let b = (sum[2] / norm).round() as u8;

            let out_idx = local_y * row_stride + (tile_x0 as usize + ox as usize) * 3;
            output[out_idx] = r;
            output[out_idx + 1] = g;
            output[out_idx + 2] = b;
        }
    }
}
