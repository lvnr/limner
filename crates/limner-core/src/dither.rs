//! Blue-noise threshold maps for dithered transparency.
//!
//! See-through strokes are drawn by keeping or discarding whole fragments against a
//! threshold. A blue-noise map spreads the kept fragments evenly, which reads as a
//! fine, even grain instead of the clumps white noise would give.

/// Builds a tileable `size` x `size` blue-noise map with the void-and-cluster
/// method. Each value from 0 to 255 appears equally often. `size` must be a power
/// of two of at least 8.
pub fn blue_noise(size: usize, seed: u64) -> Vec<u8> {
    assert!(
        size.is_power_of_two() && size >= 8,
        "size must be a power of two of at least 8"
    );
    let n = size * size;
    let mut field = Field::new(size);

    // Start from a sparse random pattern and relax it until it is evenly spread.
    let mut rng = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let initial = n / 10;
    let mut placed = 0;
    while placed < initial {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        let i = (rng % n as u64) as usize;
        if !field.on[i] {
            field.set(i, true);
            placed += 1;
        }
    }
    loop {
        let cluster = field.tightest_cluster();
        field.set(cluster, false);
        let void = field.largest_void();
        field.set(void, true);
        if void == cluster {
            break;
        }
    }

    let mut rank = vec![0usize; n];
    let prototype = field.on.clone();

    // Phase 1: rank the initial points, removing the most crowded first.
    for r in (0..initial).rev() {
        let cluster = field.tightest_cluster();
        field.set(cluster, false);
        rank[cluster] = r;
    }
    // Phase 2: refill from the prototype, always into the emptiest spot, up to half full.
    field = Field::from_pattern(size, &prototype);
    let half = n / 2;
    for r in initial..half {
        let void = field.largest_void();
        field.set(void, true);
        rank[void] = r;
    }
    // Phase 3: the empty cells are now the minority; fill where they crowd most.
    let mut inverse = Field::from_pattern(size, &field.on.iter().map(|b| !b).collect::<Vec<_>>());
    for r in half..n {
        let cluster = inverse.tightest_cluster();
        inverse.set(cluster, false);
        rank[cluster] = r;
    }

    rank.into_iter().map(|r| (r * 256 / n) as u8).collect()
}

/// A binary pattern on a torus with a running Gaussian energy map.
struct Field {
    size: usize,
    on: Vec<bool>,
    energy: Vec<f32>,
    kernel: Vec<f32>,
    radius: isize,
}

impl Field {
    fn new(size: usize) -> Self {
        const SIGMA: f32 = 1.5;
        let radius = 6isize;
        let mut kernel = Vec::with_capacity(((2 * radius + 1) * (2 * radius + 1)) as usize);
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                kernel.push((-((dx * dx + dy * dy) as f32) / (2.0 * SIGMA * SIGMA)).exp());
            }
        }
        Self {
            size,
            on: vec![false; size * size],
            energy: vec![0.0; size * size],
            kernel,
            radius,
        }
    }

    fn from_pattern(size: usize, pattern: &[bool]) -> Self {
        let mut field = Self::new(size);
        for (i, on) in pattern.iter().enumerate() {
            if *on {
                field.set(i, true);
            }
        }
        field
    }

    fn set(&mut self, index: usize, value: bool) {
        if self.on[index] == value {
            return;
        }
        self.on[index] = value;
        let sign = if value { 1.0 } else { -1.0 };
        let mask = self.size as isize - 1;
        let (x, y) = ((index % self.size) as isize, (index / self.size) as isize);
        let mut k = 0;
        for dy in -self.radius..=self.radius {
            let row = (((y + dy) & mask) as usize) * self.size;
            for dx in -self.radius..=self.radius {
                self.energy[row + ((x + dx) & mask) as usize] += sign * self.kernel[k];
                k += 1;
            }
        }
    }

    /// The set cell with the most set neighbours.
    fn tightest_cluster(&self) -> usize {
        let mut best = (f32::NEG_INFINITY, 0);
        for (i, e) in self.energy.iter().enumerate() {
            if self.on[i] && *e > best.0 {
                best = (*e, i);
            }
        }
        best.1
    }

    /// The clear cell with the fewest set neighbours.
    fn largest_void(&self) -> usize {
        let mut best = (f32::INFINITY, 0);
        for (i, e) in self.energy.iter().enumerate() {
            if !self.on[i] && *e < best.0 {
                best = (*e, i);
            }
        }
        best.1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_appears_equally_often() {
        let map = blue_noise(64, 1);
        assert_eq!(map.len(), 4096);
        let mut histogram = [0usize; 256];
        for v in &map {
            histogram[*v as usize] += 1;
        }
        assert!(histogram.iter().all(|count| *count == 16));
    }

    #[test]
    fn it_is_deterministic_and_seed_dependent() {
        assert_eq!(blue_noise(16, 7), blue_noise(16, 7));
        assert_ne!(blue_noise(16, 7), blue_noise(16, 8));
    }

    /// Thresholding at any level must leave the kept cells evenly spread: compared with
    /// white noise, far fewer kept cells may sit directly next to another kept cell.
    #[test]
    fn thresholded_patterns_are_evenly_spread() {
        let size = 64;
        let map = blue_noise(size, 3);
        let adjacent_pairs = |pattern: &dyn Fn(usize) -> bool| {
            let mut pairs = 0;
            for y in 0..size {
                for x in 0..size {
                    if pattern(y * size + x) {
                        pairs += pattern(y * size + (x + 1) % size) as usize
                            + pattern(((y + 1) % size) * size + x) as usize;
                    }
                }
            }
            pairs
        };
        // Keep one cell in eight. White noise would give about 4096 * 2 / 64 = 128 adjacent pairs.
        let blue = adjacent_pairs(&|i| map[i] < 32);
        assert!(
            blue < 16,
            "blue noise has {blue} adjacent pairs at 12.5% coverage"
        );

        // At any coverage, every 8 x 8 block should hold nearly the same number of kept
        // cells. For white noise the variance of the block counts would be 64 p (1 - p).
        for level in [64usize, 128, 192] {
            let p = level as f64 / 256.0;
            let mut counts = Vec::new();
            for by in 0..size / 8 {
                for bx in 0..size / 8 {
                    let mut count = 0;
                    for y in 0..8 {
                        for x in 0..8 {
                            count +=
                                ((map[(by * 8 + y) * size + bx * 8 + x] as usize) < level) as usize;
                        }
                    }
                    counts.push(count as f64);
                }
            }
            let mean = counts.iter().sum::<f64>() / counts.len() as f64;
            let variance =
                counts.iter().map(|c| (c - mean) * (c - mean)).sum::<f64>() / counts.len() as f64;
            assert!((mean - 64.0 * p).abs() < 1e-9);
            assert!(
                variance < 0.15 * 64.0 * p * (1.0 - p),
                "block variance {variance} at coverage {p}"
            );
        }
    }
}
