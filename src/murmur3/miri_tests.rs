use super::*;

#[test]
fn one_shot_and_incremental_boundaries_are_defined() {
    const LENGTHS: &[usize] = &[
        0, 1, 2, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257, 511,
        512, 513, 1025,
    ];
    for &length in LENGTHS {
        let input = (0..length)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(13))
            .collect::<Vec<_>>();
        for &seed in &[0, 1, u32::MAX] {
            let expected32 = murmur3_x86_32(&input, seed);
            let expected_x86 = murmur3_x86_128(&input, seed);
            let expected_x64 = murmur3_x64_128(&input, seed);
            for &chunk_size in &[1, 3, 7, 16, 31, 64] {
                let mut hash32 = Murmur3X86Hasher32::new(seed);
                let mut hash_x86 = Murmur3X86Hasher128::new(seed);
                let mut hash_x64 = Murmur3X64Hasher128::new(seed);
                for chunk in input.chunks(chunk_size) {
                    hash32.update(chunk);
                    hash_x86.update(chunk);
                    hash_x64.update(chunk);
                }
                assert_eq!(hash32.digest(), expected32);
                assert_eq!(hash_x86.digest(), expected_x86);
                assert_eq!(hash_x64.digest(), expected_x64);
            }
        }
    }
}
