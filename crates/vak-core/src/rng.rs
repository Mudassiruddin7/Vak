//! A tiny deterministic generator (SplitMix64). Perturbations are seeded from the prompt so the same
//! question always gets the same edits, which keeps audits repeatable and the answer cache useful.
//! Writing it here avoids a dependency whose API moves between releases.

use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    /// Seed from any text, stable across platforms and releases.
    pub fn from_text(namespace: &str, text: &str) -> Self {
        let mut h = Sha256::new();
        h.update(namespace.as_bytes());
        h.update([0]);
        h.update(text.as_bytes());
        let d = h.finalize();
        Rng(u64::from_be_bytes(d[..8].try_into().expect("sha256 is 32 bytes")))
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..n` (n must be > 0).
    pub fn below(&mut self, n: usize) -> usize {
        debug_assert!(n > 0);
        (self.next_u64() % n as u64) as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// `k` distinct indices out of `0..n`, in random order.
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        let mut all: Vec<usize> = (0..n).collect();
        let k = k.min(n);
        for i in 0..k {
            let j = i + self.below(n - i);
            all.swap(i, j);
        }
        all.truncate(k);
        all
    }

    pub fn chance(&mut self, p: f64) -> bool {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_stream() {
        let mut a = Rng::from_text("perturb", "ఒక కిలో బియ్యం");
        let mut b = Rng::from_text("perturb", "ఒక కిలో బియ్యం");
        assert_eq!((0..5).map(|_| a.next_u64()).collect::<Vec<_>>(), (0..5).map(|_| b.next_u64()).collect::<Vec<_>>());
        assert_ne!(Rng::from_text("x", "a").next_u64(), Rng::from_text("x", "b").next_u64());
    }

    #[test]
    fn samples_are_distinct_and_in_range() {
        let mut r = Rng::new(7);
        let s = r.sample_indices(10, 4);
        assert_eq!(s.len(), 4);
        let mut sorted = s.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 4);
        assert!(s.iter().all(|&i| i < 10));
        assert_eq!(r.sample_indices(3, 9).len(), 3);
    }
}
