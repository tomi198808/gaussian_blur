// Oktatási célú összehasonlítás: zárolás nélküli (diszjunkt szeletek) vs.
// Arc<Mutex<...>>-alapú párhuzamos Gaussian Blur.
// Részletes magyarázat: docs/mutex_demo.md

use gaussian_blur::benchmark::measure;
use gaussian_blur::blur::{gaussian_blur_3x3, gaussian_blur_3x3_tiled_mt, gaussian_blur_3x3_tiled_mutex};
use image::ImageReader;

const REPEATS: usize = 10;

fn main() -> Result<(), image::ImageError> {
    let img = ImageReader::open("images/input.png")?.decode()?;
    let rgb_img = img.to_rgb8();
    let reference = gaussian_blur_3x3(&rgb_img);

    println!("{:>6} | {:>10} | {:>10} | {:>10}", "szálak", "lock-free", "mutex", "lassulás");

    for &num_threads in &[1usize, 2, 4, 8] {
        let lock_free_result = gaussian_blur_3x3_tiled_mt(&rgb_img, num_threads);
        let mutex_result = gaussian_blur_3x3_tiled_mutex(&rgb_img, num_threads);

        for (name, result) in [("lock-free", &lock_free_result), ("mutex", &mutex_result)] {
            let mismatches = reference
                .pixels()
                .zip(result.pixels())
                .filter(|(a, b)| a != b)
                .count();
            assert_eq!(mismatches, 0, "{name} ({num_threads} szal) eltert a referenciatol!");
        }

        let lock_free_stats = measure(REPEATS, || gaussian_blur_3x3_tiled_mt(&rgb_img, num_threads));
        let mutex_stats = measure(REPEATS, || gaussian_blur_3x3_tiled_mutex(&rgb_img, num_threads));

        let slowdown = mutex_stats.median_ms() / lock_free_stats.median_ms();

        println!(
            "{:>6} | {:>8.2} ms | {:>8.2} ms | {:>9.2}x",
            num_threads,
            lock_free_stats.median_ms(),
            mutex_stats.median_ms(),
            slowdown
        );
    }

    Ok(())
}
