use super::{c_xxh3_64, c_xxh3_128};
use crate::xxhash::{xxh3_64, xxh3_128};

#[test]
fn match_empty_digests() {
    assert_eq!(xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
    assert_eq!(
        xxh3_128(b"", 0),
        [0x6001_c324_468d_497f, 0x99aa_06d3_0147_98d8]
    );
}

#[test]
fn match_long_inputs() {
    let input = (0..=2048)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
        .collect::<Vec<_>>();

    // Short inputs are checked at every alignment below.
    for length in 241..=2048 {
        let input = &input[..length];

        for &seed in &[0, 0xd6e8_feb8_6659_fd93] {
            assert_eq!(
                xxh3_64(input, seed),
                c_xxh3_64(input, seed),
                "XXH3-64 mismatch for length {length}, seed {seed:#x}",
            );
            let actual = xxh3_128(input, seed);
            assert_eq!(
                actual,
                c_xxh3_128(input, seed),
                "XXH3-128 mismatch for length {length}, seed {seed:#x}",
            );
        }
    }
}

#[test]
fn match_unaligned_inputs() {
    for offset in 0..16 {
        for length in 0..=240 {
            let owned = (0..offset + length)
                .map(|index| (index as u8).wrapping_mul(131).wrapping_add(17))
                .collect::<Vec<_>>();
            // End at the allocation boundary, including for overlapping tail loads.
            let input = &owned[offset..];

            for seed in [0, 1, 0x0123_4567_89ab_cdef, u64::MAX] {
                assert_eq!(
                    xxh3_64(input, seed),
                    c_xxh3_64(input, seed),
                    "XXH3-64 mismatch at offset {offset}, length {length}, seed {seed:#x}",
                );
                assert_eq!(
                    xxh3_128(input, seed),
                    c_xxh3_128(input, seed),
                    "XXH3-128 mismatch at offset {offset}, length {length}, seed {seed:#x}",
                );
            }
        }
    }
}

#[test]
fn match_length_boundaries() {
    const LENGTHS: &[usize] = &[
        0, 1, 2, 3, 4, 8, 9, 16, 17, 31, 32, 33, 63, 64, 65, 96, 97, 127, 128, 129, 159, 160, 191,
        192, 239, 240, 241, 255, 256, 511, 512, 1023, 1024, 1025, 4161,
    ];
    const SEEDS: &[u64] = &[0, 1, 0x0123_4567_89ab_cdef, u64::MAX];

    for &length in LENGTHS {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(131).wrapping_add(17))
            .collect();

        for &seed in SEEDS {
            assert_eq!(
                xxh3_64(&input, seed),
                xxhash_rust::xxh3::xxh3_64_with_seed(&input, seed),
                "XXH3-64 mismatch for length {length}, seed {seed:#x}",
            );
            let reference = xxhash_rust::xxh3::xxh3_128_with_seed(&input, seed);
            let actual = xxh3_128(&input, seed);
            assert_eq!(
                (u128::from(actual[1]) << 64) | u128::from(actual[0]),
                reference,
                "XXH3-128 mismatch for length {length}, seed {seed:#x}",
            );
        }
    }
}

#[test]
fn match_seeded_inputs() {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;

    for case in 0..128 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let length = (state as usize) % (128 * 1024 + 1);
        let mut input = vec![0_u8; length];

        for byte in &mut input {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *byte = state as u8;
        }

        state = state.rotate_left(29).wrapping_add(case);
        let seed = state;
        assert_eq!(xxh3_64(&input, seed), c_xxh3_64(&input, seed));
        assert_eq!(xxh3_128(&input, seed), c_xxh3_128(&input, seed));
    }
}
