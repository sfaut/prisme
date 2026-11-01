//! Fast hash for the tables of the store and of the Pivoteur (FxHash, the
//! hasher of rustc), taken from the benchmark of the engine
//! (`bench/pivot/engine`). The default hasher of the standard library
//! (SipHash) resists collision attacks, which these tables, filled from the
//! rows of a result, do not need.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hash table with the fast hasher.
pub type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// Hasher of rustc (FxHash).
#[derive(Default)]
pub struct FxHasher {
    /// Hash computed so far.
    hash: u64,
}

impl FxHasher {
    /// Mixes a word into the hash.
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    /// The rotation brings the bits mixed best, the high ones, down to where
    /// the table picks its bucket: without it, texts that differ only by
    /// their last characters (`C000123`, `C000124`) collide (fix of
    /// `rustc-hash` 2).
    fn finish(&self) -> u64 {
        self.hash.rotate_left(26)
    }
}
