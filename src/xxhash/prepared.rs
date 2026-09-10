use core::fmt;

use super::batch::hash_each_input_with_secret;
use super::long_inputs::{LongEngine, LongInput, Secret, finalize_long_64, finalize_long_128};
use super::one_shot::{xxh3_64, xxh3_128};

/// Reuses the derived XXH3 secret for repeated hashes with one seed.
///
/// Preparing a nonzero seed derives the 192-byte secret once. This avoids repeating that setup for each long input.
/// Inputs through 240 bytes use the canonical short-input formulas directly and do not read the derived secret.
///
/// # Examples
///
///     use hashcodecs::xxhash::{PreparedXxh3, xxh3_64};
///
///     let prepared = PreparedXxh3::new(42);
///     let input = vec![7; 1024];
///     assert_eq!(prepared.hash_64(&input), xxh3_64(&input, 42));
///
#[derive(Clone)]
pub struct PreparedXxh3 {
    seed: u64,
    secret: Option<Secret>,
}

impl PreparedXxh3 {
    /// Prepares one seed for repeated XXH3-64 or XXH3-128 calls.
    #[inline]
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            secret: LongEngine::cached().derive_secret(seed),
        }
    }

    /// Computes the canonical XXH3 64-bit hash with the prepared seed.
    #[inline]
    pub fn hash_64(&self, input: &[u8]) -> u64 {
        let Some(input) = LongInput::new(input) else {
            return xxh3_64(input, self.seed);
        };
        let engine = LongEngine::cached();
        engine.hash(
            input,
            engine.secret(&self.secret),
            super::long_inputs::finalize_long_64,
        )
    }

    /// Computes the canonical XXH3 128-bit hash with the prepared seed.
    #[inline]
    pub fn hash_128(&self, input: &[u8]) -> [u64; 2] {
        let Some(input) = LongInput::new(input) else {
            return xxh3_128(input, self.seed);
        };
        let engine = LongEngine::cached();
        engine.hash(
            input,
            engine.secret(&self.secret),
            super::long_inputs::finalize_long_128,
        )
    }

    /// Computes canonical XXH3 64-bit hashes with the prepared seed.
    ///
    /// The result order matches the input order. Eligible long inputs share the
    /// prepared secret and the same grouped scheduler as [`super::xxh3_64_batch`].
    #[inline]
    pub fn hash_64_batch(&self, inputs: &[&[u8]]) -> Vec<u64> {
        let mut hashes = Vec::with_capacity(inputs.len());
        self.hash_64_batch_for_each(inputs, |hash| hashes.push(hash));
        hashes
    }

    /// Computes canonical XXH3 64-bit hashes and sends each result to a callback.
    ///
    /// This method does not allocate a result vector. It calls `output` once
    /// for each input, in input order.
    #[inline]
    pub fn hash_64_batch_for_each(&self, inputs: &[&[u8]], output: impl FnMut(u64)) {
        let engine = LongEngine::cached();
        hash_each_input_with_secret(
            inputs,
            self.seed,
            xxh3_64,
            finalize_long_64,
            engine,
            engine.secret(&self.secret),
            output,
        );
    }

    /// Computes canonical XXH3 128-bit hashes with the prepared seed.
    ///
    /// The result order matches the input order. Each result contains the
    /// `[low64, high64]` word pair returned by [`super::xxh3_128_batch`].
    #[inline]
    pub fn hash_128_batch(&self, inputs: &[&[u8]]) -> Vec<[u64; 2]> {
        let mut hashes = Vec::with_capacity(inputs.len());
        self.hash_128_batch_for_each(inputs, |hash| hashes.push(hash));
        hashes
    }

    /// Computes canonical XXH3 128-bit hashes and sends each result to a callback.
    ///
    /// This method does not allocate a result vector. It calls `output` once
    /// for each input, in input order.
    #[inline]
    pub fn hash_128_batch_for_each(&self, inputs: &[&[u8]], output: impl FnMut([u64; 2])) {
        let engine = LongEngine::cached();
        hash_each_input_with_secret(
            inputs,
            self.seed,
            xxh3_128,
            finalize_long_128,
            engine,
            engine.secret(&self.secret),
            output,
        );
    }
}

impl fmt::Debug for PreparedXxh3 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedXxh3")
            .field("seed", &self.seed)
            .finish_non_exhaustive()
    }
}

impl Default for PreparedXxh3 {
    fn default() -> Self {
        Self::new(0)
    }
}
