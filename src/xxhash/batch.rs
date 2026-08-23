#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{self, SimdBackend};

use super::hash::{xxh3_64_with_long_secret, xxh3_128_with_long_secret};
use super::long::{finalize_long_128, init_secret, merge};
use super::primitives::{P64_1, SECRET};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86;

#[inline]
fn batch4_long_accumulators(chunk: &[&[u8]], secret: &[u8]) -> Option<[[u64; 8]; 4]> {
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let _ = (chunk, secret);
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if chunk[0].len() > 240
        && chunk.iter().all(|input| input.len() == chunk[0].len())
        && backend::capabilities().supports(SimdBackend::Avx2)
    {
        let values = [chunk[0], chunk[1], chunk[2], chunk[3]];
        return Some(unsafe { x86::avx2::long_accumulate_batch4(values, secret) });
    }
    None
}

/// Computes canonical XXH3 64-bit hashes for a batch without copying inputs.
///
/// Results preserve input order. Seed-derived setup is shared by the batch, and
/// equal-size long inputs may be processed four at a time on AVX2.
///
/// # Arguments
///
/// * inputs - Borrowed byte slices to hash in order.
/// * seed - The initial unsigned 64-bit seed shared by every input.
///
/// # Returns
///
/// One canonical 64-bit hash per input.
///
/// # Examples
///
///     use hashcodecs::xxhash::{xxh3_64, xxh3_64_batch};
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     assert_eq!(
///         xxh3_64_batch(inputs, 7),
///         inputs.iter().map(|input| xxh3_64(input, 7)).collect::<Vec<_>>(),
///     );
///
#[inline]
pub fn xxh3_64_batch(inputs: &[&[u8]], seed: u64) -> Vec<u64> {
    let owned_secret = (seed != 0).then(|| init_secret(seed));
    let secret = owned_secret.as_ref().unwrap_or(&SECRET);
    let mut output = Vec::with_capacity(inputs.len());
    let (chunks, remainder) = inputs.as_chunks::<4>();
    for chunk in chunks {
        if let Some(accumulators) = batch4_long_accumulators(chunk, secret) {
            output.extend(accumulators.iter().map(|acc| {
                merge(
                    acc,
                    &secret[11..],
                    (chunk[0].len() as u64).wrapping_mul(P64_1),
                )
            }));
            continue;
        }
        output.extend(
            chunk
                .iter()
                .map(|input| xxh3_64_with_long_secret(input, seed, secret)),
        );
    }
    output.extend(
        remainder
            .iter()
            .map(|input| xxh3_64_with_long_secret(input, seed, secret)),
    );
    output
}
/// Computes canonical XXH3 128-bit hashes for a batch without copying inputs.
///
/// Results preserve input order. Seed-derived setup is shared by the batch, and
/// equal-size long inputs may be processed four at a time on AVX2.
///
/// # Arguments
///
/// * inputs - Borrowed byte slices to hash in order.
/// * seed - The initial unsigned 64-bit seed shared by every input.
///
/// # Returns
///
/// One `[low64, high64]` word pair per input. Each pair follows the same
/// contract as [`crate::xxhash::xxh3_128`].
///
/// # Examples
///
///     use hashcodecs::xxhash::{xxh3_128, xxh3_128_batch};
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     assert_eq!(
///         xxh3_128_batch(inputs, 7),
///         inputs.iter().map(|input| xxh3_128(input, 7)).collect::<Vec<_>>(),
///     );
///
#[inline]
pub fn xxh3_128_batch(inputs: &[&[u8]], seed: u64) -> Vec<[u64; 2]> {
    let owned_secret = (seed != 0).then(|| init_secret(seed));
    let secret = owned_secret.as_ref().unwrap_or(&SECRET);
    let mut output = Vec::with_capacity(inputs.len());
    let (chunks, remainder) = inputs.as_chunks::<4>();
    for chunk in chunks {
        if let Some(accumulators) = batch4_long_accumulators(chunk, secret) {
            let length = chunk[0].len();
            output.extend(
                accumulators
                    .into_iter()
                    .map(|acc| finalize_long_128(length, secret, acc)),
            );
            continue;
        }
        output.extend(
            chunk
                .iter()
                .map(|input| xxh3_128_with_long_secret(input, seed, secret)),
        );
    }
    output.extend(
        remainder
            .iter()
            .map(|input| xxh3_128_with_long_secret(input, seed, secret)),
    );
    output
}
