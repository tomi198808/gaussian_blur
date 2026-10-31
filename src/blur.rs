//! Gaussian Blur — három egyszálú implementáció: direkt (naiv) 2D konvolúció,
//! szeparábilis (két 1D lépéses), és csempézett (tiled), kígyózó bejárású,
//! cache-optimalizált változat. Mindhárom pixelre pontosan ugyanazt az
//! eredményt adja vissza -- ezt a `main.rs` programból is ellenőrzi.
//!
//! Részletes magyarázat: docs/blur.md

use image::{ImageBuffer, Rgb, RgbImage};
use std::sync::{Arc, Mutex};
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_add_ps, _mm_mul_ps, _mm_set1_ps, _mm_set_ps, _mm_storeu_ps};

/// 3x3-as Gauss-kernel: `[1,2,1; 2,4,2; 1,2,1]`, a normalizáláshoz a
/// [`KERNEL_3X3_SUM`]-mal osztva.
pub const KERNEL_3X3: [[f32; 3]; 3] = [
    [1.0, 2.0, 1.0],
    [2.0, 4.0, 2.0],
    [1.0, 2.0, 1.0],
];

/// A [`KERNEL_3X3`] súlyainak összege (16).
pub const KERNEL_3X3_SUM: f32 = 16.0;

/// Az 1D Gauss-kernel, amiből a [`KERNEL_3X3`] szeparábilis (outer product)
/// felbontással előáll: `[1,2,1] ⊗ [1,2,1] = KERNEL_3X3`.
pub const KERNEL_1D: [f32; 3] = [1.0, 2.0, 1.0];

/// A [`KERNEL_1D`] súlyainak összege (4).
pub const KERNEL_1D_SUM: f32 = 4.0;

/// Lebegőpontos (f32) pixeleket tároló kép, a szeparábilis implementáció
/// köztes eredményéhez -- hogy a két lépés között ne veszítsek pontosságot.
type FloatImage = ImageBuffer<Rgb<f32>, Vec<f32>>;

/// Elvégzi a 3x3-as Gaussian Blur-t egy RGB8 képen, direkt 2D konvolúcióval,
/// egyetlen szálon. Ez a REFERENCIA implementáció, amihez a többi változat
/// eredményét hasonlítom.
pub fn gaussian_blur_3x3(input: &RgbImage) -> RgbImage {
    let (width, height) = input.dimensions();
    let mut output = RgbImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];

            for ky in 0..3i64 {
                for kx in 0..3i64 {
                    let sample_x = x as i64 + kx - 1;
                    let sample_y = y as i64 + ky - 1;
                    let px = sample_x.clamp(0, width as i64 - 1) as u32;
                    let py = sample_y.clamp(0, height as i64 - 1) as u32;

                    let pixel = input.get_pixel(px, py);
                    let weight = KERNEL_3X3[ky as usize][kx as usize];

                    for c in 0..3usize {
                        sum[c] += pixel[c] as f32 * weight;
                    }
                }
            }

            let r = (sum[0] / KERNEL_3X3_SUM).round() as u8;
            let g = (sum[1] / KERNEL_3X3_SUM).round() as u8;
            let b = (sum[2] / KERNEL_3X3_SUM).round() as u8;

            output.put_pixel(x, y, Rgb([r, g, b]));
        }
    }

    output
}

/// A szeparábilis Gaussian Blur első lépése: minden sort önmagában,
/// vízszintesen elmos. Kerekítés nélkül ad vissza [`FloatImage`]-et, hogy a
/// második lépés után a végeredmény bit-pontosan megegyezzen a direkt 2D
/// konvolúcióéval.
fn horizontal_pass(input: &RgbImage) -> FloatImage {
    let (width, height) = input.dimensions();
    let mut output = FloatImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];

            for kx in 0..3i64 {
                let sample_x = (x as i64 + kx - 1).clamp(0, width as i64 - 1) as u32;
                let pixel = input.get_pixel(sample_x, y);
                let weight = KERNEL_1D[kx as usize];
                for c in 0..3usize {
                    sum[c] += pixel[c] as f32 * weight;
                }
            }

            output.put_pixel(x, y, Rgb(sum));
        }
    }

    output
}

/// A szeparábilis Gaussian Blur második lépése: a vízszintesen már elmosott
/// köztes eredményt függőlegesen is elmossa, és itt kerekít vissza `u8`-ra.
fn vertical_pass(input: &FloatImage) -> RgbImage {
    let (width, height) = input.dimensions();
    let mut output = RgbImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let mut sum = [0.0f32; 3];

            for ky in 0..3i64 {
                let sample_y = (y as i64 + ky - 1).clamp(0, height as i64 - 1) as u32;
                let pixel = input.get_pixel(x, sample_y);
                let weight = KERNEL_1D[ky as usize];
                for c in 0..3usize {
                    sum[c] += pixel[c] * weight;
                }
            }

            let r = (sum[0] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            let g = (sum[1] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            let b = (sum[2] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;

            output.put_pixel(x, y, Rgb([r, g, b]));
        }
    }

    output
}

/// Szeparábilis (két 1D lépéses) Gaussian Blur: vízszintes, majd függőleges
/// elmosás. Ugyanazt az eredményt adja, mint [`gaussian_blur_3x3`].
pub fn gaussian_blur_3x3_separable(input: &RgbImage) -> RgbImage {
    let intermediate = horizontal_pass(input);
    vertical_pass(&intermediate)
}

/// Egy csempe (tile) mérete pixelben -- lásd docs/blur.md az L1 cache alapú
/// indoklásért.
pub const TILE_SIZE: u32 = 16;

/// Csempézett (tiled), kígyózó (boustrophedon) bejárású, cache-optimalizált
/// szeparábilis Gaussian Blur, EGYETLEN szálon. Ugyanazt az eredményt adja,
/// mint [`gaussian_blur_3x3`].
pub fn gaussian_blur_3x3_tiled(input: &RgbImage) -> RgbImage {
    let (width, height) = input.dimensions();
    let mut output = RgbImage::new(width, height);
    let tiles_y = height.div_ceil(TILE_SIZE);
    let row_stride = width as usize * 3;

    let raw: &mut [u8] = &mut output;
    process_tile_row_range(input, raw, row_stride, 0, width, 0, tiles_y);

    output
}

/// Ugyanaz, mint [`gaussian_blur_3x3_tiled`], de a csempesorokat `num_threads`
/// szál között osztja szét, kontiguus (összefüggő) sávokban.
pub fn gaussian_blur_3x3_tiled_mt(input: &RgbImage, num_threads: usize) -> RgbImage {
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
                );
            });
        }
    });

    output
}

/// Feldolgozza a `[tile_row_start, tile_row_end)` csempesor-tartományt
/// kígyózó bejárással, mindegyik csempét az [`process_tile`] hívásával
/// számolva ki.
fn process_tile_row_range(
    input: &RgbImage,
    output: &mut [u8],
    row_stride: usize,
    row_offset: u32,
    width: u32,
    tile_row_start: u32,
    tile_row_end: u32,
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

            process_tile(input, output, row_stride, row_offset, tile_x0, tile_y0, tile_w, tile_h);
        }
    }
}

/// Egy kernel-tag hozzáadása mindhárom (R,G,B) csatorna részösszegéhez,
/// explicit SSE2 SIMD-del (128 bites, 4×f32 regiszter). Lásd docs/blur.md.
#[cfg(target_arch = "x86_64")]
#[inline]
fn accumulate_channels(sum: &mut [f32; 3], values: [f32; 3], weight: f32) {
    // SAFETY: SSE2 az x86-64 garantált alapja, nincs szükség feature-detektálásra.
    unsafe {
        let weight_vec = _mm_set1_ps(weight);
        let values_vec = _mm_set_ps(0.0, values[2], values[1], values[0]);
        let sum_vec = _mm_set_ps(0.0, sum[2], sum[1], sum[0]);
        let result = _mm_add_ps(sum_vec, _mm_mul_ps(values_vec, weight_vec));

        let mut out = [0.0f32; 4];
        _mm_storeu_ps(out.as_mut_ptr(), result);
        sum[0] = out[0];
        sum[1] = out[1];
        sum[2] = out[2];
    }
}

/// Ugyanaz, mint a fenti SIMD-es `accumulate_channels`, sima skalár
/// ciklussal -- nem x86-64 architektúrákon (pl. ARM) ez fordul be.
#[cfg(not(target_arch = "x86_64"))]
#[inline]
fn accumulate_channels(sum: &mut [f32; 3], values: [f32; 3], weight: f32) {
    for c in 0..3usize {
        sum[c] += values[c] * weight;
    }
}

/// Feldolgoz egyetlen csempét: elvégzi rajta a teljes szeparábilis
/// (vízszintes + függőleges) Gaussian Blur-t, az eredményt az `output` nyers
/// bájtpufferbe írva. Az `output` a teljes kép egy sor-igazított darabja is
/// lehet (`row_offset` mondja meg, az `output[0]` melyik globális sornak
/// felel meg) -- ez teszi lehetővé, hogy több szál a kép DISZJUNKT
/// szeleteire írjon egyidejűleg. A csempe a saját +1 pixeles "halo" peremét
/// is újraszámolja, hogy más csempétől függetlenül működhessen.
fn process_tile(
    input: &RgbImage,
    output: &mut [u8],
    row_stride: usize,
    row_offset: u32,
    tile_x0: u32,
    tile_y0: u32,
    tile_w: u32,
    tile_h: u32,
) {
    let (width, height) = input.dimensions();
    let padded_h = tile_h + 2;
    let mut intermediate = vec![0.0f32; (tile_w as usize) * (padded_h as usize) * 3];

    // Vízszintes elmosás a csempe + halo soraira.
    for ky in 0..padded_h {
        let y = (tile_y0 as i64 + ky as i64 - 1).clamp(0, height as i64 - 1) as u32;

        for kx in 0..tile_w {
            let x = tile_x0 + kx;
            let mut sum = [0.0f32; 3];

            for dx in 0..3i64 {
                let sample_x = (x as i64 + dx - 1).clamp(0, width as i64 - 1) as u32;
                let pixel = input.get_pixel(sample_x, y);
                let weight = KERNEL_1D[dx as usize];
                let values = [pixel[0] as f32, pixel[1] as f32, pixel[2] as f32];
                accumulate_channels(&mut sum, values, weight);
            }

            let base = ((ky * tile_w + kx) * 3) as usize;
            intermediate[base] = sum[0];
            intermediate[base + 1] = sum[1];
            intermediate[base + 2] = sum[2];
        }
    }

    // Függőleges elmosás a köztes pufferen, majd írás a nyers kimeneti bájtokba.
    for oy in 0..tile_h {
        let local_y = (tile_y0 + oy - row_offset) as usize;

        for ox in 0..tile_w {
            let mut sum = [0.0f32; 3];

            for dy in 0..3u32 {
                let iy = oy + dy;
                let base = ((iy * tile_w + ox) * 3) as usize;
                let weight = KERNEL_1D[dy as usize];
                let values = [intermediate[base], intermediate[base + 1], intermediate[base + 2]];
                accumulate_channels(&mut sum, values, weight);
            }

            let r = (sum[0] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            let g = (sum[1] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            let b = (sum[2] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;

            let out_idx = local_y * row_stride + (tile_x0 as usize + ox as usize) * 3;
            output[out_idx] = r;
            output[out_idx + 1] = g;
            output[out_idx + 2] = b;
        }
    }
}

// ======================================================================
// Oktatási célú, Arc<Mutex<...>>-alapú változat -- lásd docs/mutex_demo.md
// ======================================================================

/// Kiszámolja egyetlen csempe pixeleit egy önálló, helyi `Vec<u8>` bufferbe,
/// zárolás nélkül.
fn compute_tile_pixels(input: &RgbImage, tile_x0: u32, tile_y0: u32, tile_w: u32, tile_h: u32) -> Vec<u8> {
    let (width, height) = input.dimensions();
    let padded_h = tile_h + 2;
    let mut intermediate = vec![0.0f32; (tile_w as usize) * (padded_h as usize) * 3];

    for ky in 0..padded_h {
        let y = (tile_y0 as i64 + ky as i64 - 1).clamp(0, height as i64 - 1) as u32;
        for kx in 0..tile_w {
            let x = tile_x0 + kx;
            let mut sum = [0.0f32; 3];
            for dx in 0..3i64 {
                let sample_x = (x as i64 + dx - 1).clamp(0, width as i64 - 1) as u32;
                let pixel = input.get_pixel(sample_x, y);
                let weight = KERNEL_1D[dx as usize];
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

    let mut tile_pixels = vec![0u8; (tile_w as usize) * (tile_h as usize) * 3];
    for oy in 0..tile_h {
        for ox in 0..tile_w {
            let mut sum = [0.0f32; 3];
            for dy in 0..3u32 {
                let iy = oy + dy;
                let base = ((iy * tile_w + ox) * 3) as usize;
                let weight = KERNEL_1D[dy as usize];
                sum[0] += intermediate[base] * weight;
                sum[1] += intermediate[base + 1] * weight;
                sum[2] += intermediate[base + 2] * weight;
            }
            let out_base = ((oy * tile_w + ox) * 3) as usize;
            tile_pixels[out_base] = (sum[0] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            tile_pixels[out_base + 1] = (sum[1] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
            tile_pixels[out_base + 2] = (sum[2] / (KERNEL_1D_SUM * KERNEL_1D_SUM)).round() as u8;
        }
    }

    tile_pixels
}

/// Ugyanaz, mint [`gaussian_blur_3x3_tiled_mt`], de a kimeneti képet
/// `Arc<Mutex<RgbImage>>`-ba csomagolva, MINDEN csempe után zárolva írja.
/// Szándékosan lassabb -- lásd docs/mutex_demo.md a lock contention magyarázatért.
pub fn gaussian_blur_3x3_tiled_mutex(input: &RgbImage, num_threads: usize) -> RgbImage {
    let (width, height) = input.dimensions();
    let output = Arc::new(Mutex::new(RgbImage::new(width, height)));

    let tiles_x = width.div_ceil(TILE_SIZE);
    let tiles_y = height.div_ceil(TILE_SIZE);
    let tile_rows_per_thread = tiles_y.div_ceil(num_threads as u32);

    std::thread::scope(|scope| {
        for t in 0..num_threads as u32 {
            let tile_row_start = t * tile_rows_per_thread;
            let tile_row_end = (tile_row_start + tile_rows_per_thread).min(tiles_y);
            let output = Arc::clone(&output);

            scope.spawn(move || {
                for tile_row in tile_row_start..tile_row_end {
                    for tile_col in 0..tiles_x {
                        let tile_x0 = tile_col * TILE_SIZE;
                        let tile_y0 = tile_row * TILE_SIZE;
                        let tile_w = TILE_SIZE.min(width - tile_x0);
                        let tile_h = TILE_SIZE.min(height - tile_y0);

                        let tile_pixels = compute_tile_pixels(input, tile_x0, tile_y0, tile_w, tile_h);

                        let mut guard = output.lock().unwrap();
                        for oy in 0..tile_h {
                            for ox in 0..tile_w {
                                let idx = ((oy * tile_w + ox) * 3) as usize;
                                let pixel = Rgb([
                                    tile_pixels[idx],
                                    tile_pixels[idx + 1],
                                    tile_pixels[idx + 2],
                                ]);
                                guard.put_pixel(tile_x0 + ox, tile_y0 + oy, pixel);
                            }
                        }
                    }
                }
            });
        }
    });

    Arc::try_unwrap(output).unwrap().into_inner().unwrap()
}
