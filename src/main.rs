// Részletes magyarázat: docs/main.md

use gaussian_blur::benchmark::measure;
use gaussian_blur::blur::{
    gaussian_blur_3x3, gaussian_blur_3x3_separable, gaussian_blur_3x3_tiled,
    gaussian_blur_3x3_tiled_mt,
};
use image::ImageReader;
use image::imageops::{self, FilterType};
use std::time::Instant;

/// Hányszor ismétlem meg egy-egy szálszám mérését a statisztikákhoz.
const BENCHMARK_REPEATS: usize = 15;

/// Betölti az `images/input.png` képet, lefuttatja mindhárom egyszálú Gaussian
/// Blur implementációt, leméri és keresztellenőrzi őket, majd elmenti az
/// eredményt és egy nagyított összehasonlító kivágást.
fn main() -> Result<(), image::ImageError> {
    let path = "images/input.png";
    let reader = ImageReader::open(path)?;
    let img = reader.decode()?;

    let width = img.width();
    let height = img.height();
    let color_type = img.color();

    println!("Kép betöltve: {path}");
    println!("Szélesség:    {width} px");
    println!("Magasság:     {height} px");
    println!("Pixel formátum (ColorType): {color_type:?}");

    let rgb_img = img.to_rgb8();

    let start = Instant::now();
    let blurred = gaussian_blur_3x3(&rgb_img);
    let elapsed = start.elapsed();
    println!("Gaussian Blur (3x3, 1 szál) futási ideje: {} ms", elapsed.as_millis());

    let start_separable = Instant::now();
    let blurred_separable = gaussian_blur_3x3_separable(&rgb_img);
    let elapsed_separable = start_separable.elapsed();
    println!(
        "Gaussian Blur (3x3, szeparabilis, 1 szal) futasi ideje: {} ms",
        elapsed_separable.as_millis()
    );

    let mut mismatched_pixels: u64 = 0;
    for (direct_px, separable_px) in blurred.pixels().zip(blurred_separable.pixels()) {
        if direct_px != separable_px {
            mismatched_pixels += 1;
        }
    }
    println!("Direkt vs. szeparabilis: {mismatched_pixels} pixel ter el egymastol (a celom: 0)");
    assert_eq!(
        mismatched_pixels, 0,
        "A szeparabilis implementacio MAS eredmenyt adott, mint a direkt referencia!"
    );

    let start_tiled = Instant::now();
    let blurred_tiled = gaussian_blur_3x3_tiled(&rgb_img);
    let elapsed_tiled = start_tiled.elapsed();
    println!(
        "Gaussian Blur (3x3, csempezett+kigyozo, 1 szal) futasi ideje: {} ms",
        elapsed_tiled.as_millis()
    );

    let mut mismatched_tiled: u64 = 0;
    for (direct_px, tiled_px) in blurred.pixels().zip(blurred_tiled.pixels()) {
        if direct_px != tiled_px {
            mismatched_tiled += 1;
        }
    }
    println!("Direkt vs. csempezett: {mismatched_tiled} pixel ter el egymastol (a celom: 0)");
    assert_eq!(
        mismatched_tiled, 0,
        "A csempezett implementacio MAS eredmenyt adott, mint a direkt referencia!"
    );

    println!(
        "Logikai CPU magok (available_parallelism): {}",
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    );

    println!(
        "\nBenchmark: {BENCHMARK_REPEATS} ismétlés szálszámonként (2048x2048, 3x3 kernel, csempézett algoritmus)"
    );
    println!("{:>6} | {:>8} | {:>8} | {:>8} | {:>8} | {:>8} | {:>6}",
        "szálak", "min ms", "max ms", "átlag ms", "medián ms", "speedup", "haték.");

    let mut baseline_median_ms: Option<f64> = None;

    for &num_threads in &[1usize, 2, 4, 8] {
        // Korrektségi ellenőrzés -- EGYETLEN, nem mért futással, KÜLÖN a
        // teljesítménymérés ismételt hívásaitól.
        let blurred_mt = gaussian_blur_3x3_tiled_mt(&rgb_img, num_threads);
        let mut mismatched_mt: u64 = 0;
        for (direct_px, mt_px) in blurred.pixels().zip(blurred_mt.pixels()) {
            if direct_px != mt_px {
                mismatched_mt += 1;
            }
        }
        assert_eq!(
            mismatched_mt, 0,
            "A {num_threads} szalas csempezett implementacio MAS eredmenyt adott, mint a referencia!"
        );

        if num_threads > 1 {
            let mt_output_path = format!("images/output_{num_threads}_threads.png");
            blurred_mt.save(&mt_output_path)?;
        }

        // A tényleges, ismételt teljesítménymérés.
        let stats = measure(BENCHMARK_REPEATS, || gaussian_blur_3x3_tiled_mt(&rgb_img, num_threads));
        let median_ms = stats.median_ms();

        let baseline = *baseline_median_ms.get_or_insert(median_ms);
        let speedup = baseline / median_ms;
        let efficiency = speedup / num_threads as f64 * 100.0;

        println!(
            "{:>6} | {:>8.2} | {:>8.2} | {:>8.2} | {:>8.2} | {:>7.2}x | {:>5.1}%",
            num_threads,
            stats.min.as_secs_f64() * 1000.0,
            stats.max.as_secs_f64() * 1000.0,
            stats.mean.as_secs_f64() * 1000.0,
            median_ms,
            speedup,
            efficiency
        );
    }

    let output_path = "images/output_single.png";
    blurred.save(output_path)?;
    println!("Kimenet elmentve: {output_path}");

    // Sanity check: bizonyítom, hogy a blur ténylegesen változtatott valamit.
    let mut changed_pixels: u64 = 0;
    let mut max_channel_diff: i32 = 0;
    for (orig, new) in rgb_img.pixels().zip(blurred.pixels()) {
        let mut pixel_changed = false;
        for c in 0..3usize {
            let diff = (orig[c] as i32 - new[c] as i32).abs();
            if diff > max_channel_diff {
                max_channel_diff = diff;
            }
            if diff != 0 {
                pixel_changed = true;
            }
        }
        if pixel_changed {
            changed_pixels += 1;
        }
    }
    let total_pixels = width as u64 * height as u64;
    println!(
        "Ellenorzes: {changed_pixels} / {total_pixels} pixel valtozott, legnagyobb csatorna-elteres: {max_channel_diff}"
    );
    assert!(
        changed_pixels > 0,
        "A Gaussian Blur nem valtoztatott egyetlen pixelen sem -- ez hibara utal!"
    );

    // Nagyított, éles sakktábla-határra eső kivágás a vizuális ellenőrzéshez.
    let (crop_x, crop_y, crop_size) = (992u32, 1056u32, 64u32);
    let crop_before = imageops::crop_imm(&rgb_img, crop_x, crop_y, crop_size, crop_size).to_image();
    let crop_after = imageops::crop_imm(&blurred, crop_x, crop_y, crop_size, crop_size).to_image();

    let zoom = 8;
    let crop_before = imageops::resize(&crop_before, crop_size * zoom, crop_size * zoom, FilterType::Nearest);
    let crop_after = imageops::resize(&crop_after, crop_size * zoom, crop_size * zoom, FilterType::Nearest);

    crop_before.save("images/crop_before.png")?;
    crop_after.save("images/crop_after.png")?;
    println!("Nagyitott osszehasonlito kivagasok elmentve: images/crop_before.png, images/crop_after.png");

    Ok(())
}
