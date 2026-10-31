// Segédprogram: éles fekete-fehér sakktábla-mintás teszt PNG generálása.
// Részletes magyarázat: docs/gen_test_image.md

use image::{Rgb, RgbImage};

fn main() {
    let width = 2048;
    let height = 2048;
    let mut img = RgbImage::new(width, height);
    let square_size = 128u32;

    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let is_white = ((x / square_size) + (y / square_size)) % 2 == 0;
        let color = if is_white {
            Rgb([255u8, 255u8, 255u8])
        } else {
            Rgb([0u8, 0u8, 0u8])
        };
        *pixel = color;
    }

    img.save("images/input.png")
        .expect("Nem sikerult elmenteni a teszt kepet (images/input.png)");

    println!("Teszt kep elkeszult: images/input.png ({width}x{height})");
}
