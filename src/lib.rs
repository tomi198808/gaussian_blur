//! Library crate: a `blur` és `benchmark` modulokat teszi elérhetővé a
//! `src/main.rs` fő program ÉS a `src/bin/`-ben lévő önálló segédprogramok
//! (pl. `gen_test_image`, `mutex_demo`) számára is. Egy csomagban (package)
//! egyszerre lehet egy library crate (ez a fájl) ÉS tetszőleges számú binary
//! crate (`src/main.rs` + `src/bin/*.rs`) -- a bináris crate-ek a csomag
//! nevén (itt: `gaussian_blur`) keresztül `use gaussian_blur::...`-tal érik
//! el a library nyilvános elemeit.

pub mod benchmark;
pub mod blur;
pub mod blur_nxn;
