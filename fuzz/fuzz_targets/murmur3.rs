#![no_main]

use std::io::Cursor;

use hashcodecs::{
    Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128, murmur3_x86_32,
    murmur3_x86_128,
};
use libfuzzer_sys::fuzz_target;

const MAX_INPUT: usize = 1024 * 1024;

fuzz_target!(|bytes: &[u8]| {
    let Some((seed_bytes, input)) = bytes.split_at_checked(4) else {
        return;
    };
    let input = &input[..input.len().min(MAX_INPUT)];
    let seed = u32::from_le_bytes(seed_bytes.try_into().expect("four-byte seed"));

    let expected32 = murmur3::murmur3_32(&mut Cursor::new(input), seed).expect("in-memory input");
    assert_eq!(murmur3_x86_32(input, seed), expected32);

    let expected_x86 =
        murmur3::murmur3_x86_128(&mut Cursor::new(input), seed).expect("in-memory input");
    let actual_x86 = murmur3_x86_128(input, seed);
    assert_eq!(
        u128::from(actual_x86[0])
            | (u128::from(actual_x86[1]) << 32)
            | (u128::from(actual_x86[2]) << 64)
            | (u128::from(actual_x86[3]) << 96),
        expected_x86
    );

    let expected_x64 =
        murmur3::murmur3_x64_128(&mut Cursor::new(input), seed).expect("in-memory input");
    let actual_x64 = murmur3_x64_128(input, seed);
    assert_eq!(
        u128::from(actual_x64[0]) | (u128::from(actual_x64[1]) << 64),
        expected_x64
    );

    let chunk_size = (seed as usize % 127) + 1;
    let mut incremental32 = Murmur3X86Hasher32::new(seed);
    let mut incremental_x86 = Murmur3X86Hasher128::new(seed);
    let mut incremental_x64 = Murmur3X64Hasher128::new(seed);
    for chunk in input.chunks(chunk_size) {
        incremental32.update(chunk);
        incremental_x86.update(chunk);
        incremental_x64.update(chunk);
    }
    assert_eq!(incremental32.digest(), murmur3_x86_32(input, seed));
    assert_eq!(incremental_x86.digest(), murmur3_x86_128(input, seed));
    assert_eq!(incremental_x64.digest(), murmur3_x64_128(input, seed));
});
