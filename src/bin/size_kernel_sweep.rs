// Méret- és kernelméret-sweep benchmark: 512..8192 képméretek x 3x3..9x9
// kernelek x 1/2/4/8 szál. Az eredményt egy markdown táblázatba is elmenti
// (docs/sweep_results.md), hogy közvetlenül felhasználható legyen a
// beadandó dokumentációban.
// Részletes magyarázat: docs/size_kernel_sweep.md

use gaussian_blur::benchmark::measure;
use gaussian_blur::blur_nxn::{binomial_kernel_1d, gaussian_blur_direct, gaussian_blur_tiled_mt};
use image::{Rgb, RgbImage};
use std::fmt::Write as _;
use std::fs;

const REPEATS: usize = 5;
const SIZES: [u32; 5] = [512, 1024, 2048, 4096, 8192];
const RADII: [usize; 4] = [1, 2, 3, 4]; // 3x3, 5x5, 7x7, 9x9
const THREAD_COUNTS: [usize; 4] = [1, 2, 4, 8];

fn generate_checkerboard(size: u32) -> RgbImage {
    let mut img = RgbImage::new(size, size);
    let square_size = 64u32;
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let is_white = ((x / square_size) + (y / square_size)) % 2 == 0;
        *pixel = if is_white { Rgb([255, 255, 255]) } else { Rgb([0, 0, 0]) };
    }
    img
}

fn main() {
    // Korrektségi ellenőrzés: KERNELMÉRETENKÉNT egyszer, egy kis (512x512)
    // képen. A csempézés/halo logika helyessége nem függ a kép méretétől,
    // csak a kernel méretétől -- ezért nem szükséges minden (méret, kernel)
    // kombinációnál újra lefuttatni a lassú, direkt O(k^2) referenciát.
    println!("Korrektségi ellenőrzés kernelméretenként (512x512 teszt kép)...");
    let correctness_image = generate_checkerboard(512);
    for &radius in &RADII {
        let kernel = binomial_kernel_1d(radius);
        let reference = gaussian_blur_direct(&correctness_image, &kernel);

        for &threads in &THREAD_COUNTS {
            let result = gaussian_blur_tiled_mt(&correctness_image, &kernel, threads);
            let mismatches = reference.pixels().zip(result.pixels()).filter(|(a, b)| a != b).count();
            assert_eq!(
                mismatches, 0,
                "Kernel {}x{}, {threads} szal: {mismatches} pixel elteres!",
                kernel.len(), kernel.len()
            );
        }
    }
    println!("Minden kernelméret (3x3, 5x5, 7x7, 9x9) 0 eltéréssel OK.\n");

    let mut report = String::new();
    writeln!(report, "# Méret- és kernelméret-sweep eredmények\n").unwrap();
    writeln!(
        report,
        "Csempézett + kígyózó, szálankénti diszjunkt szeletekre bontott algoritmus, \
         {REPEATS} ismétlés/kombináció, medián alapján.\n"
    ).unwrap();

    for &size in &SIZES {
        println!("=== Kép: {size}x{size} ===");
        let img = generate_checkerboard(size);

        writeln!(report, "## Kép: {size}x{size}\n").unwrap();
        writeln!(report, "| Kernel | Szálak | Medián (ms) | Speedup | Hatékonyság |").unwrap();
        writeln!(report, "|---|---|---|---|---|").unwrap();

        for &radius in &RADII {
            let kernel = binomial_kernel_1d(radius);
            let kernel_size = kernel.len();
            let mut baseline_ms: Option<f64> = None;

            for &threads in &THREAD_COUNTS {
                let stats = measure(REPEATS, || gaussian_blur_tiled_mt(&img, &kernel, threads));
                let median_ms = stats.median_ms();
                let baseline = *baseline_ms.get_or_insert(median_ms);
                let speedup = baseline / median_ms;
                let efficiency = speedup / threads as f64 * 100.0;

                println!(
                    "  {kernel_size}x{kernel_size} kernel, {threads:>2} szál: {median_ms:>8.2} ms  \
                     (speedup {speedup:>5.2}x, hatékonyság {efficiency:>5.1}%)"
                );
                writeln!(
                    report,
                    "| {kernel_size}x{kernel_size} | {threads} | {median_ms:.2} | {speedup:.2}x | {efficiency:.1}% |"
                ).unwrap();
            }
        }
        writeln!(report).unwrap();
    }

    fs::write("docs/sweep_results.md", &report).expect("Nem sikerult elmenteni: docs/sweep_results.md");
    println!("\nEredmények elmentve: docs/sweep_results.md");
}
