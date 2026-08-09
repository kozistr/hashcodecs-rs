//! MurmurHash3 reference-compatible functions with runtime SIMD dispatch.
//!
//! SIMD kernels premix independent input words in parallel. The canonical
//! loop-carried state transitions remain ordered exactly as specified.

#[inline(always)]
fn fmix32(mut hash: u32) -> u32 {
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ (hash >> 16)
}

#[inline(always)]
fn fmix64(mut hash: u64) -> u64 {
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

#[inline(always)]
fn read_u16_le(input: &[u8], offset: usize) -> u16 {
    debug_assert!(offset + 2 <= input.len());
    unsafe { u16::from_le((input.as_ptr().add(offset).cast::<u16>()).read_unaligned()) }
}

#[inline(always)]
fn read_u32_le(input: &[u8], offset: usize) -> u32 {
    debug_assert!(offset + 4 <= input.len());
    // Bounds are checked by the caller. Unaligned reads avoid a temporary
    // array and compile to one load on the x86 CPUs this crate targets.
    unsafe { u32::from_le((input.as_ptr().add(offset).cast::<u32>()).read_unaligned()) }
}

#[inline(always)]
fn read_u64_le(input: &[u8], offset: usize) -> u64 {
    debug_assert!(offset + 8 <= input.len());
    // See `read_u32_le`; this is portable because `from_le` normalizes endian.
    unsafe { u64::from_le((input.as_ptr().add(offset).cast::<u64>()).read_unaligned()) }
}

#[inline(always)]
fn read_partial_u64_le(input: &[u8]) -> u64 {
    debug_assert!(input.len() <= 8);
    match input.len() {
        0 => 0,
        1 => input[0] as u64,
        2 => read_u16_le(input, 0) as u64,
        3 => read_u16_le(input, 0) as u64 | ((input[2] as u64) << 16),
        4 => read_u32_le(input, 0) as u64,
        5 => read_u32_le(input, 0) as u64 | ((input[4] as u64) << 32),
        6 => read_u32_le(input, 0) as u64 | ((read_u16_le(input, 4) as u64) << 32),
        7 => {
            read_u32_le(input, 0) as u64
                | ((read_u16_le(input, 4) as u64) << 32)
                | ((input[6] as u64) << 48)
        }
        _ => {
            debug_assert_eq!(input.len(), 8);
            read_u64_le(input, 0)
        }
    }
}

/// Computes the canonical MurmurHash3 x86 32-bit hash.
#[inline]
pub fn murmur3_x86_32(key: &[u8], seed: u32) -> u32 {
    #[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
    {
        if key.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            return unsafe { x86::murmur3_x86_32_avx2(key, seed) };
        }
        if key.len() >= 16 && std::is_x86_feature_detected!("sse4.1") {
            return unsafe { x86::murmur3_x86_32_sse41(key, seed) };
        }
    }
    murmur3_x86_32_scalar(key, seed)
}

#[inline]
fn murmur3_x86_32_scalar(key: &[u8], seed: u32) -> u32 {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;

    let mut hash = seed;
    let block_end = key.len() & !3;
    let mut offset = 0;
    while offset < block_end {
        let block = read_u32_le(key, offset)
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        hash = mix_x86_32_hash(hash, block);
        offset += 4;
    }

    finish_x86_32(key, hash, offset)
}

#[inline(always)]
fn mix_x86_32_hash(mut hash: u32, block: u32) -> u32 {
    hash ^= block;
    hash.rotate_left(13)
        .wrapping_mul(5)
        .wrapping_add(0xe654_6b64)
}

#[inline(always)]
fn finish_x86_32(key: &[u8], mut hash: u32, offset: usize) -> u32 {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;
    let tail_len = key.len() & 3;
    let mut tail = 0u32;
    if tail_len == 3 {
        tail ^= (key[offset + 2] as u32) << 16;
    }
    if tail_len >= 2 {
        tail ^= (key[offset + 1] as u32) << 8;
    }
    if tail_len != 0 {
        tail ^= key[offset] as u32;
        hash ^= tail.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
    }

    fmix32(hash ^ key.len() as u32)
}

/// Computes the canonical MurmurHash3 x86 128-bit hash as four `u32` words.
#[inline]
pub fn murmur3_x86_128(key: &[u8], seed: u32) -> [u32; 4] {
    let mut hashes = [seed; 4];
    let block_end = key.len() & !15;
    mix_x86_128_body(&key[..block_end], &mut hashes);
    finish_x86_128(key, hashes, block_end)
}

#[inline]
fn finish_x86_128(key: &[u8], mut hashes: [u32; 4], offset: usize) -> [u32; 4] {
    let tail = &key[offset..];
    let mut blocks = [0u32; 4];
    for (index, byte) in tail.iter().copied().enumerate() {
        blocks[index / 4] |= (byte as u32) << ((index % 4) * 8);
    }
    if tail.len() > 12 {
        hashes[3] ^= blocks[3]
            .wrapping_mul(0xa1e3_8b93)
            .rotate_left(18)
            .wrapping_mul(0x239b_961b);
    }
    if tail.len() > 8 {
        hashes[2] ^= blocks[2]
            .wrapping_mul(0x38b3_4ae5)
            .rotate_left(17)
            .wrapping_mul(0xa1e3_8b93);
    }
    if tail.len() > 4 {
        hashes[1] ^= blocks[1]
            .wrapping_mul(0xab0e_9789)
            .rotate_left(16)
            .wrapping_mul(0x38b3_4ae5);
    }
    if !tail.is_empty() {
        hashes[0] ^= blocks[0]
            .wrapping_mul(0x239b_961b)
            .rotate_left(15)
            .wrapping_mul(0xab0e_9789);
    }

    finalize_x86_128(hashes, key.len() as u32)
}

/// Computes the canonical MurmurHash3 x64 128-bit hash as two `u64` words.
#[inline(always)]
pub fn murmur3_x64_128(key: &[u8], seed: u32) -> [u64; 2] {
    murmur3_x64_128_inner(key, seed as u64)
}

#[inline(never)]
fn murmur3_x64_128_inner(key: &[u8], seed: u64) -> [u64; 2] {
    #[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
    {
        let block_end = key.len() & !15;
        if key.len() >= 32 && std::is_x86_feature_detected!("avx2") {
            let hashes = unsafe { x86::mix_x64_128_body_avx2(&key[..block_end], seed) };
            return finish_x64_128(key, hashes, block_end);
        }
        if key.len() >= 16
            && key.len() <= 8 * 1024 * 1024
            && std::is_x86_feature_detected!("sse4.1")
        {
            let hashes = unsafe { x86::mix_x64_128_body_sse41(&key[..block_end], seed) };
            return finish_x64_128(key, hashes, block_end);
        }
    }
    murmur3_x64_128_scalar_inner(key, seed)
}

#[inline(never)]
fn murmur3_x64_128_scalar_inner(key: &[u8], seed: u64) -> [u64; 2] {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;

    let mut hash1 = seed;
    let mut hash2 = seed;
    let block_end = key.len() & !15;
    let mut input = key.as_ptr();
    let end = unsafe { input.add(block_end) };
    while input < end {
        let value1 = u64::from_le(unsafe { input.cast::<u64>().read_unaligned() });
        let value2 = u64::from_le(unsafe { input.add(8).cast::<u64>().read_unaligned() });
        let block1 = value1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        let block2 = value2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        hash1 ^= block1;
        hash1 = hash1
            .rotate_left(27)
            .wrapping_add(hash2)
            .wrapping_mul(5)
            .wrapping_add(0x52dc_e729);
        hash2 ^= block2;
        hash2 = hash2
            .rotate_left(31)
            .wrapping_add(hash1)
            .wrapping_mul(5)
            .wrapping_add(0x3849_5ab5);
        input = unsafe { input.add(16) };
    }

    let tail = &key[block_end..];
    if tail.len() > 8 {
        let block2 = read_partial_u64_le(&tail[8..]);
        hash2 ^= block2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    }
    if !tail.is_empty() {
        let block1 = read_partial_u64_le(&tail[..tail.len().min(8)]);
        hash1 ^= block1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
    }

    let length = key.len() as u64;
    hash1 ^= length;
    hash2 ^= length;
    hash1 = hash1.wrapping_add(hash2);
    hash2 = hash2.wrapping_add(hash1);
    hash1 = fmix64(hash1);
    hash2 = fmix64(hash2);
    hash1 = hash1.wrapping_add(hash2);
    hash2 = hash2.wrapping_add(hash1);
    [hash1, hash2]
}

#[inline]
fn finish_x64_128(key: &[u8], mut hashes: [u64; 2], offset: usize) -> [u64; 2] {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    let tail = &key[offset..];
    if tail.len() > 8 {
        let block2 = read_partial_u64_le(&tail[8..]);
        hashes[1] ^= block2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    }
    if !tail.is_empty() {
        let block1 = read_partial_u64_le(&tail[..tail.len().min(8)]);
        hashes[0] ^= block1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
    }

    let length = key.len() as u64;
    hashes[0] ^= length;
    hashes[1] ^= length;
    hashes[0] = hashes[0].wrapping_add(hashes[1]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[0] = fmix64(hashes[0]);
    hashes[1] = fmix64(hashes[1]);
    hashes[0] = hashes[0].wrapping_add(hashes[1]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes
}

#[inline]
fn mix_x86_128_body(key: &[u8], hashes: &mut [u32; 4]) {
    #[cfg(all(not(coverage), any(target_arch = "x86", target_arch = "x86_64")))]
    {
        if key.len() >= 256 && std::is_x86_feature_detected!("avx2") {
            unsafe { x86::mix_x86_128_body_avx2(key, hashes) };
            return;
        }
        if key.len() >= 16 * 1024 * 1024 && std::is_x86_feature_detected!("sse4.1") {
            unsafe { x86::mix_x86_128_body_sse41(key, hashes) };
            return;
        }
    }
    mix_x86_128_body_scalar(key, hashes);
}

#[inline]
fn mix_x86_128_body_scalar(key: &[u8], hashes: &mut [u32; 4]) {
    const C1: [u32; 4] = [0x239b_961b, 0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93];
    const C2: [u32; 4] = [0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93, 0x239b_961b];
    const ROTATE_K: [u32; 4] = [15, 16, 17, 18];

    let mut offset = 0;
    while offset < key.len() {
        let block1 = read_u32_le(key, offset)
            .wrapping_mul(C1[0])
            .rotate_left(ROTATE_K[0])
            .wrapping_mul(C2[0]);
        let block2 = read_u32_le(key, offset + 4)
            .wrapping_mul(C1[1])
            .rotate_left(ROTATE_K[1])
            .wrapping_mul(C2[1]);
        let block3 = read_u32_le(key, offset + 8)
            .wrapping_mul(C1[2])
            .rotate_left(ROTATE_K[2])
            .wrapping_mul(C2[2]);
        let block4 = read_u32_le(key, offset + 12)
            .wrapping_mul(C1[3])
            .rotate_left(ROTATE_K[3])
            .wrapping_mul(C2[3]);
        mix_x86_128_hashes(hashes, block1, block2, block3, block4);
        offset += 16;
    }
}

#[inline(always)]
fn mix_x86_128_hashes(hashes: &mut [u32; 4], block1: u32, block2: u32, block3: u32, block4: u32) {
    const ROTATE_H: [u32; 4] = [19, 17, 15, 13];
    const ADD: [u32; 4] = [0x561c_cd1b, 0x0bca_a747, 0x96cd_1c35, 0x32ac_3b17];

    hashes[0] ^= block1;
    hashes[0] = hashes[0]
        .rotate_left(ROTATE_H[0])
        .wrapping_add(hashes[1])
        .wrapping_mul(5)
        .wrapping_add(ADD[0]);
    hashes[1] ^= block2;
    hashes[1] = hashes[1]
        .rotate_left(ROTATE_H[1])
        .wrapping_add(hashes[2])
        .wrapping_mul(5)
        .wrapping_add(ADD[1]);
    hashes[2] ^= block3;
    hashes[2] = hashes[2]
        .rotate_left(ROTATE_H[2])
        .wrapping_add(hashes[3])
        .wrapping_mul(5)
        .wrapping_add(ADD[2]);
    hashes[3] ^= block4;
    hashes[3] = hashes[3]
        .rotate_left(ROTATE_H[3])
        .wrapping_add(hashes[0])
        .wrapping_mul(5)
        .wrapping_add(ADD[3]);
}

#[inline]
fn finalize_x86_128(mut hashes: [u32; 4], length: u32) -> [u32; 4] {
    for hash in &mut hashes {
        *hash ^= length;
    }
    hashes[0] = hashes[0]
        .wrapping_add(hashes[1])
        .wrapping_add(hashes[2])
        .wrapping_add(hashes[3]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[2] = hashes[2].wrapping_add(hashes[0]);
    hashes[3] = hashes[3].wrapping_add(hashes[0]);
    for hash in &mut hashes {
        *hash = fmix32(*hash);
    }
    hashes[0] = hashes[0]
        .wrapping_add(hashes[1])
        .wrapping_add(hashes[2])
        .wrapping_add(hashes[3]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[2] = hashes[2].wrapping_add(hashes[0]);
    hashes[3] = hashes[3].wrapping_add(hashes[0]);
    hashes
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn x86_words_as_u128(words: [u32; 4]) -> u128 {
        let mut bytes = [0; 16];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        u128::from_le_bytes(bytes)
    }

    fn x64_words_as_u128(words: [u64; 2]) -> u128 {
        (words[0] as u128) | ((words[1] as u128) << 64)
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn assert_x86_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
        let block_end = input.len() & !15;
        #[cfg(coverage)]
        unsafe {
            let mut sse = [seed; 4];
            x86::mix_x86_128_body_sse41(&input[..block_end], &mut sse);
            assert_eq!(
                x86_words_as_u128(finish_x86_128(input, sse, block_end)),
                expected
            );

            let mut avx = [seed; 4];
            x86::mix_x86_128_body_avx2(&input[..block_end], &mut avx);
            assert_eq!(
                x86_words_as_u128(finish_x86_128(input, avx, block_end)),
                expected
            );
        }
        #[cfg(not(coverage))]
        unsafe {
            if std::is_x86_feature_detected!("sse4.1") {
                let mut sse = [seed; 4];
                x86::mix_x86_128_body_sse41(&input[..block_end], &mut sse);
                assert_eq!(
                    x86_words_as_u128(finish_x86_128(input, sse, block_end)),
                    expected
                );
            }
            if std::is_x86_feature_detected!("avx2") {
                let mut avx = [seed; 4];
                x86::mix_x86_128_body_avx2(&input[..block_end], &mut avx);
                assert_eq!(
                    x86_words_as_u128(finish_x86_128(input, avx, block_end)),
                    expected
                );
            }
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn assert_x64_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
        let block_end = input.len() & !15;
        #[cfg(coverage)]
        unsafe {
            let sse = x86::mix_x64_128_body_sse41(&input[..block_end], seed as u64);
            assert_eq!(
                x64_words_as_u128(finish_x64_128(input, sse, block_end)),
                expected
            );

            let avx = x86::mix_x64_128_body_avx2(&input[..block_end], seed as u64);
            assert_eq!(
                x64_words_as_u128(finish_x64_128(input, avx, block_end)),
                expected
            );
        }
        #[cfg(not(coverage))]
        unsafe {
            if std::is_x86_feature_detected!("sse4.1") {
                let sse = x86::mix_x64_128_body_sse41(&input[..block_end], seed as u64);
                assert_eq!(
                    x64_words_as_u128(finish_x64_128(input, sse, block_end)),
                    expected
                );
            }
            if std::is_x86_feature_detected!("avx2") {
                let avx = x86::mix_x64_128_body_avx2(&input[..block_end], seed as u64);
                assert_eq!(
                    x64_words_as_u128(finish_x64_128(input, avx, block_end)),
                    expected
                );
            }
        }
    }

    #[test]
    fn known_answer_vectors() {
        assert_eq!(read_partial_u64_le(&[]), 0);
        assert_eq!(murmur3_x86_32(b"hello", 0), 0x248b_fa47);
        assert_eq!(murmur3_x86_32(&[1, 2, 3], 0), 2_161_234_436);
        assert_eq!(
            murmur3_x86_128(&[1, 2, 3], 0),
            [4_127_286_497, 3_037_938_227, 3_037_938_227, 3_037_938_227]
        );
        assert_eq!(
            murmur3_x64_128(&[1, 2, 3], 0),
            [1_901_714_139_111_438_249, 5_282_241_052_699_499_109]
        );
    }

    #[test]
    fn scalar_x86_128_body_matches_the_reference() {
        let data: Vec<u8> = (0..255).map(|value| value as u8).collect();
        let mut scalar = [7; 4];
        mix_x86_128_body(&data[..240], &mut scalar);
        let scalar_hash = finalize_x86_128(scalar, 240);
        assert_eq!(
            x86_words_as_u128(scalar_hash),
            murmur3::murmur3_x86_128(&mut Cursor::new(&data[..240]), 7).unwrap()
        );
    }

    #[test]
    fn matches_the_reference_implementation_for_every_tail_length() {
        let seeds = [0, 1, 0xfeed_beef, u32::MAX];
        for length in 0..=512 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(73).wrapping_add(19))
                .collect();
            for seed in seeds {
                let expected_x86_32 = murmur3::murmur3_32(&mut Cursor::new(&input), seed).unwrap();
                assert_eq!(
                    murmur3_x86_32(&input, seed),
                    expected_x86_32,
                    "x86_32 length={length} seed={seed}"
                );
                assert_eq!(
                    murmur3_x86_32_scalar(&input, seed),
                    expected_x86_32,
                    "scalar x86_32 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                unsafe {
                    #[cfg(coverage)]
                    {
                        assert_eq!(
                            x86::murmur3_x86_32_sse41(&input, seed),
                            expected_x86_32,
                            "SSE4.1 x86_32 length={length} seed={seed}"
                        );
                        assert_eq!(
                            x86::murmur3_x86_32_avx2(&input, seed),
                            expected_x86_32,
                            "AVX2 x86_32 length={length} seed={seed}"
                        );
                    }
                    #[cfg(not(coverage))]
                    if std::is_x86_feature_detected!("sse4.1") {
                        assert_eq!(
                            x86::murmur3_x86_32_sse41(&input, seed),
                            expected_x86_32,
                            "SSE4.1 x86_32 length={length} seed={seed}"
                        );
                    }
                    #[cfg(not(coverage))]
                    if std::is_x86_feature_detected!("avx2") {
                        assert_eq!(
                            x86::murmur3_x86_32_avx2(&input, seed),
                            expected_x86_32,
                            "AVX2 x86_32 length={length} seed={seed}"
                        );
                    }
                }

                let expected_x86_128 =
                    murmur3::murmur3_x86_128(&mut Cursor::new(&input), seed).unwrap();
                let x86 = x86_words_as_u128(murmur3_x86_128(&input, seed));
                assert_eq!(x86, expected_x86_128, "x86_128 length={length} seed={seed}");
                let block_end = input.len() & !15;
                let mut scalar_x86_128 = [seed; 4];
                mix_x86_128_body_scalar(&input[..block_end], &mut scalar_x86_128);
                assert_eq!(
                    x86_words_as_u128(finish_x86_128(&input, scalar_x86_128, block_end)),
                    expected_x86_128,
                    "scalar x86_128 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                assert_x86_128_simd_backends(&input, seed, expected_x86_128);

                let x64 = x64_words_as_u128(murmur3_x64_128(&input, seed));
                let expected_x64_128 =
                    murmur3::murmur3_x64_128(&mut Cursor::new(&input), seed).unwrap();
                assert_eq!(x64, expected_x64_128, "x64_128 length={length} seed={seed}");
                assert_eq!(
                    x64_words_as_u128(murmur3_x64_128_scalar_inner(&input, seed as u64)),
                    expected_x64_128,
                    "scalar x64_128 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                assert_x64_128_simd_backends(&input, seed, expected_x64_128);
            }
        }
    }
}
