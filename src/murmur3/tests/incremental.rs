use super::x64_words_as_u128;
use crate::murmur3::{
    Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128, murmur3_x86_32,
    murmur3_x86_128,
};
use std::io::Cursor;

#[test]
fn match_simd_updates() {
    let input: Vec<u8> = (0..2048)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(19))
        .collect();

    for seed in [0, 1, 0xfeed_beef, u32::MAX] {
        let expected = murmur3::murmur3_x64_128(&mut Cursor::new(&input), seed).unwrap();

        for chunk_size in [496, 511, 512, 513, 528, 1024] {
            for prefix_length in 0..16 {
                let mut hasher = Murmur3X64Hasher128::new(seed);
                hasher.update(&input[..prefix_length]);

                for chunk in input[prefix_length..].chunks(chunk_size) {
                    hasher.update(chunk);
                }

                assert_eq!(
                    x64_words_as_u128(hasher.digest()),
                    expected,
                    "seed={seed} chunk_size={chunk_size} prefix_length={prefix_length}"
                );
            }
        }
    }
}

#[test]
fn match_chunked_updates() {
    let seeds = [0, u32::MAX];
    let chunk_sizes = [1, 2, 3, 4, 7, 16, 31, 64];

    for length in 0..=128 {
        let input: Vec<u8> = (0..length)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(7))
            .collect();

        for seed in seeds {
            for chunk_size in chunk_sizes {
                let mut x86_32 = Murmur3X86Hasher32::new(seed);
                let mut x86_128 = Murmur3X86Hasher128::new(seed);
                let mut x64_128 = Murmur3X64Hasher128::new(seed);

                for chunk in input.chunks(chunk_size) {
                    x86_32.update(chunk);
                    x86_128.update(chunk);
                    x64_128.update(chunk);
                }

                assert_eq!(x86_32.digest(), murmur3_x86_32(&input, seed));
                assert_eq!(x86_128.digest(), murmur3_x86_128(&input, seed));
                assert_eq!(x64_128.digest(), murmur3_x64_128(&input, seed));
            }
        }
    }
}

#[test]
fn use_zero_seed() {
    assert_eq!(
        Murmur3X86Hasher32::default().digest(),
        murmur3_x86_32(b"", 0)
    );
    assert_eq!(
        Murmur3X86Hasher128::default().digest(),
        murmur3_x86_128(b"", 0)
    );
    assert_eq!(
        Murmur3X64Hasher128::default().digest(),
        murmur3_x64_128(b"", 0)
    );
}

#[test]
fn clone_partial_blocks() {
    macro_rules! check {
        ($hasher:ty, $one_shot:path) => {
            for length in [3, 4, 15, 16, 17, 31] {
                let prefix = vec![b'p'; length];
                let mut original = <$hasher>::new(42);
                original.update(&prefix);
                let mut snapshot = original.clone();

                // Digest and empty updates must leave a pending block intact.
                assert_eq!(original.digest(), $one_shot(&prefix, 42));
                original.update(b"");
                original.update(b"-original");
                snapshot.update(b"-snapshot");

                assert_eq!(
                    original.digest(),
                    $one_shot(&[prefix.as_slice(), b"-original"].concat(), 42)
                );
                assert_eq!(
                    snapshot.digest(),
                    $one_shot(&[prefix.as_slice(), b"-snapshot"].concat(), 42)
                );
            }
        };
    }

    check!(Murmur3X86Hasher32, murmur3_x86_32);
    check!(Murmur3X86Hasher128, murmur3_x86_128);
    check!(Murmur3X64Hasher128, murmur3_x64_128);
}

#[test]
fn redact_buffered_input() {
    let secret = b"secret message bytes";

    let mut x86_32 = Murmur3X86Hasher32::new(7);
    let mut x86_128 = Murmur3X86Hasher128::new(7);
    let mut x64_128 = Murmur3X64Hasher128::new(7);
    x86_32.update(secret);
    x86_128.update(secret);
    x64_128.update(secret);

    assert_eq!(
        format!("{x86_32:?}"),
        "Murmur3Hasher { algorithm: \"x86_32\", total_length: 20, buffered_length: 0 }"
    );
    assert_eq!(
        format!("{x86_128:?}"),
        "Murmur3Hasher { algorithm: \"x86_128\", total_length: 20, buffered_length: 4 }"
    );
    assert_eq!(
        format!("{x64_128:?}"),
        "Murmur3Hasher { algorithm: \"x64_128\", total_length: 20, buffered_length: 4 }"
    );
}
