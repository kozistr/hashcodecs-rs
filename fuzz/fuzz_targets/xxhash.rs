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

    // Independent heterogeneous allocations exercise every batch remainder,
    // including more than one complete four-lane group.
    let mut owned = std::array::from_fn::<_, 9, _>(|index| {
        let start = input.len().saturating_mul(index) / 9;
        input[start..].to_vec()
    });
    for (index, value) in owned.iter_mut().enumerate() {
        if let Some(first) = value.first_mut() {
            *first = first.wrapping_add(index as u8);
        }
        if let Some(last) = value.last_mut() {
            *last ^= (index as u8).wrapping_mul(0x5b);
        }
    }
    let batch = owned.each_ref().map(Vec::as_slice);
    for width in 1..=batch.len() {
        let batch = &batch[..width];
        assert_eq!(
            xxh3_64_batch(batch, seed),
            batch
                .iter()
                .map(|value| xxhash_rust::xxh3::xxh3_64_with_seed(value, seed))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            xxh3_128_batch(batch, seed),
            batch
                .iter()
                .map(|value| {
                    let hash = xxhash_rust::xxh3::xxh3_128_with_seed(value, seed);
                    [hash as u64, (hash >> 64) as u64]
                })
                .collect::<Vec<_>>()
        );
    }
});
