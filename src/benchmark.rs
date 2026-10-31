//! Benchmark segédeszközök: egy művelet többszöri megismétlése és a
//! futási idők statisztikai összegzése (min/max/átlag/medián).
//!
//! Részletes magyarázat: docs/benchmark.md

use std::time::{Duration, Instant};

/// Egy megismételt mérés statisztikai összegzése.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    pub min: Duration,
    pub max: Duration,
    pub mean: Duration,
    pub median: Duration,
}

impl Stats {
    /// A medián ezredmásodpercben (ms), `f64`-ként -- ez a speedup/hatékonyság
    /// számításának alapja.
    pub fn median_ms(&self) -> f64 {
        self.median.as_secs_f64() * 1000.0
    }
}

/// Lefuttatja az `f` closure-t `repeats` alkalommal, méri mindegyik futási
/// idejét, és visszaadja a statisztikai összegzést. Az `f` visszatérési
/// értékét minden alkalommal eldobja (csak az időt tartja meg) -- a
/// korrektségi ellenőrzést a hívónak KÜLÖN, a mérés előtt/után kell
/// elvégeznie, hogy a mérés valóban csak a futási időt tükrözze.
pub fn measure<F, R>(repeats: usize, mut f: F) -> Stats
where
    F: FnMut() -> R,
{
    let mut samples: Vec<Duration> = Vec::with_capacity(repeats);

    for _ in 0..repeats {
        let start = Instant::now();
        let result = f();
        let elapsed = start.elapsed();
        std::hint::black_box(&result);
        samples.push(elapsed);
    }

    samples.sort();

    let min = samples[0];
    let max = samples[samples.len() - 1];
    let mean = samples.iter().copied().sum::<Duration>() / samples.len() as u32;
    let mid = samples.len() / 2;
    let median = if samples.len() % 2 == 0 {
        (samples[mid - 1] + samples[mid]) / 2
    } else {
        samples[mid]
    };

    Stats { min, max, mean, median }
}
