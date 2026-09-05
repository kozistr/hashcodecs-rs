#![no_main]

use hashcodecs::xxhash::{xxh3_64, xxh3_64_batch, xxh3_128, xxh3_128_batch};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 1024 * 1024;

fuzz_target!(|bytes: &[u8]| {
    let Some((seed_bytes, input)) = bytes.split_at_checked(8) else {
        return;
    };
    let input = &input[..input.len().min(MAX_INPUT)];
    let seed = u64::from_le_bytes(seed_bytes.try_into().expect("eight-byte seed"));

    assert_eq!(
        xxh3_64(input, seed),
        xxhash_rust::xxh3::xxh3_64_with_seed(input, seed)
    );
    let expected128 = xxhash_rust::xxh3::xxh3_128_with_seed(input, seed);
    assert_eq!(
        xxh3_128(input, seed),
        [expected128 as u64, (expected128 >> 64) as u64]
    );

    // Equal-length long inputs reach the SIMD batch kernels; heterogeneous
    // inputs exercise run splitting. Keep each lane in an independent allocation.
    for equal_length in [true, false] {
        let owned = std::array::from_fn::<_, 9, _>(|index| {
            let start = if equal_length {
                0
            } else {
                input.len().saturating_mul(index) / 9
            };
            let mut value = input[start..]
                .iter()
                .map(|byte| byte.wrapping_add(index as u8))
                .collect::<Vec<_>>();
            // Guarantee distinct contents even for uniform or empty fuzz input.
            let first = input
                .first()
                .copied()
                .unwrap_or(0)
                .wrapping_add(index as u8);
            if value.is_empty() {
                value.push(first);
            } else {
                value[0] = first;
            }
            value
        });
        let batch = owned.each_ref().map(Vec::as_slice);
        let expected64 = batch
            .iter()
            .map(|value| xxhash_rust::xxh3::xxh3_64_with_seed(value, seed))
            .collect::<Vec<_>>();
        let expected128 = batch
            .iter()
            .map(|value| {
                let hash = xxhash_rust::xxh3::xxh3_128_with_seed(value, seed);
                [hash as u64, (hash >> 64) as u64]
            })
            .collect::<Vec<_>>();
        // Includes two- and three-lane kernels, four-lane groups, and tails
        // after one or two complete groups.
        for width in 1..=batch.len() {
            assert_eq!(xxh3_64_batch(&batch[..width], seed), expected64[..width]);
            assert_eq!(xxh3_128_batch(&batch[..width], seed), expected128[..width]);
        }
    }
});
