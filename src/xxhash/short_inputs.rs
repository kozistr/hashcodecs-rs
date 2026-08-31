//! Apply XXH3 formulas to inputs that contain at most 240 bytes.

use super::primitives::*;

pub(super) fn xxh3_64_len_0_to_16(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    if len == 0 {
        return avalanche64(seed ^ (read_u64_le(&SECRET, 56) ^ read_u64_le(&SECRET, 64)));
    }
    if len <= 3 {
        let combined = (input[0] as u32) << 16
            | (input[len >> 1] as u32) << 24
            | input[len - 1] as u32
            | (len as u32) << 8;
        return avalanche64(
            (combined as u64)
                ^ ((read_u32_le(&SECRET, 0) ^ read_u32_le(&SECRET, 4)) as u64).wrapping_add(seed),
        );
    }
    if len <= 8 {
        let seed = seed ^ ((seed as u32).swap_bytes() as u64) << 32;
        let input_word = read_u32_le(input, len - 4) as u64 | (read_u32_le(input, 0) as u64) << 32;
        return rrmxmx(
            input_word ^ (read_u64_le(&SECRET, 8) ^ read_u64_le(&SECRET, 16)).wrapping_sub(seed),
            len,
        );
    }
    let lo = read_u64_le(input, 0)
        ^ (read_u64_le(&SECRET, 24) ^ read_u64_le(&SECRET, 32)).wrapping_add(seed);
    let hi = read_u64_le(input, len - 8)
        ^ (read_u64_le(&SECRET, 40) ^ read_u64_le(&SECRET, 48)).wrapping_sub(seed);
    avalanche(
        (len as u64)
            .wrapping_add(lo.swap_bytes())
            .wrapping_add(hi)
            .wrapping_add(mul_fold(lo, hi)),
    )
}

pub(super) fn xxh3_64_len_17_to_128(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut acc = (len as u64).wrapping_mul(P64_1);
    if len > 32 {
        if len > 64 {
            if len > 96 {
                acc = acc.wrapping_add(mix16(input, 48, &SECRET, 96, seed));
                acc = acc.wrapping_add(mix16(input, len - 64, &SECRET, 112, seed));
            }
            acc = acc.wrapping_add(mix16(input, 32, &SECRET, 64, seed));
            acc = acc.wrapping_add(mix16(input, len - 48, &SECRET, 80, seed));
        }
        acc = acc.wrapping_add(mix16(input, 16, &SECRET, 32, seed));
        acc = acc.wrapping_add(mix16(input, len - 32, &SECRET, 48, seed));
    }
    acc = acc.wrapping_add(mix16(input, 0, &SECRET, 0, seed));
    acc = acc.wrapping_add(mix16(input, len - 16, &SECRET, 16, seed));
    avalanche(acc)
}

pub(super) fn xxh3_64_len_129_to_240(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut acc = (len as u64).wrapping_mul(P64_1);
    for i in 0..8 {
        acc = acc.wrapping_add(mix16(input, 16 * i, &SECRET, 16 * i, seed));
    }
    acc = avalanche(acc);
    for i in 8..(len / 16) {
        acc = acc.wrapping_add(mix16(input, 16 * i, &SECRET, 3 + 16 * (i - 8), seed));
    }
    avalanche(acc.wrapping_add(mix16(input, len - 16, &SECRET, 119, seed)))
}

pub(super) fn xxh3_128_len_64(input: &[u8], seed: u64) -> [u64; 2] {
    let acc = [64_u64.wrapping_mul(P64_1), 0];
    let acc = mix32(acc, input, 16, 32, 32, seed);
    final128(mix32(acc, input, 0, 48, 0, seed), 64, seed)
}

pub(super) fn xxh3_128_len_0_to_16(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();
    if len == 0 {
        return [
            avalanche64(seed ^ (read_u64_le(&SECRET, 64) ^ read_u64_le(&SECRET, 72))),
            avalanche64(seed ^ (read_u64_le(&SECRET, 80) ^ read_u64_le(&SECRET, 88))),
        ];
    }
    if len <= 3 {
        let c = (input[0] as u32) << 16
            | (input[len >> 1] as u32) << 24
            | input[len - 1] as u32
            | (len as u32) << 8;
        let h = c.swap_bytes().rotate_left(13);
        return [
            avalanche64(
                c as u64
                    ^ ((read_u32_le(&SECRET, 0) ^ read_u32_le(&SECRET, 4)) as u64)
                        .wrapping_add(seed),
            ),
            avalanche64(
                h as u64
                    ^ ((read_u32_le(&SECRET, 8) ^ read_u32_le(&SECRET, 12)) as u64)
                        .wrapping_sub(seed),
            ),
        ];
    }
    if len <= 8 {
        let seed = seed ^ ((seed as u32).swap_bytes() as u64) << 32;
        let input_word = read_u32_le(input, 0) as u64 | (read_u32_le(input, len - 4) as u64) << 32;
        let keyed =
            input_word ^ (read_u64_le(&SECRET, 16) ^ read_u64_le(&SECRET, 24)).wrapping_add(seed);
        let product = (keyed as u128) * (P64_1.wrapping_add((len as u64) << 2) as u128);
        let mut lo = product as u64;
        let mut hi = (product >> 64) as u64;
        hi = hi.wrapping_add(lo << 1);
        lo ^= hi >> 3;
        lo ^= lo >> 35;
        lo = lo.wrapping_mul(MX2);
        lo ^= lo >> 28;
        return [lo, avalanche(hi)];
    }
    let lo = read_u64_le(input, 0);
    let mut hi = read_u64_le(input, len - 8);
    let product =
        ((lo ^ hi ^ (read_u64_le(&SECRET, 32) ^ read_u64_le(&SECRET, 40)).wrapping_sub(seed))
            as u128)
            * P64_1 as u128;
    let mut low = (product as u64).wrapping_add(((len - 1) as u64) << 54);
    let mut high = (product >> 64) as u64;
    hi ^= (read_u64_le(&SECRET, 48) ^ read_u64_le(&SECRET, 56)).wrapping_add(seed);
    high = high.wrapping_add(hi.wrapping_add((hi as u32 as u64).wrapping_mul(P32_2 - 1)));
    low ^= high.swap_bytes();
    let product = (low as u128) * P64_2 as u128;
    [
        avalanche(product as u64),
        avalanche(((product >> 64) as u64).wrapping_add(high.wrapping_mul(P64_2))),
    ]
}

pub(super) fn mix32(
    mut acc: [u64; 2],
    input: &[u8],
    first: usize,
    second: usize,
    secret: usize,
    seed: u64,
) -> [u64; 2] {
    acc[0] = acc[0].wrapping_add(mix16(input, first, &SECRET, secret, seed));
    acc[0] ^= read_u64_le(input, second).wrapping_add(read_u64_le(input, second + 8));
    acc[1] = acc[1].wrapping_add(mix16(input, second, &SECRET, secret + 16, seed));
    acc[1] ^= read_u64_le(input, first).wrapping_add(read_u64_le(input, first + 8));
    acc
}

pub(super) fn final128(acc: [u64; 2], len: usize, seed: u64) -> [u64; 2] {
    [
        avalanche(acc[0].wrapping_add(acc[1])),
        0u64.wrapping_sub(avalanche(
            acc[0]
                .wrapping_mul(P64_1)
                .wrapping_add(acc[1].wrapping_mul(P64_4))
                .wrapping_add((len as u64).wrapping_sub(seed).wrapping_mul(P64_2)),
        )),
    ]
}

pub(super) fn xxh3_128_len_17_to_128(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];
    for i in (0..=((len - 1) / 32)).rev() {
        acc = mix32(acc, input, i * 16, len - 16 * (i + 1), i * 32, seed);
    }
    final128(acc, len, seed)
}

pub(super) fn xxh3_128_len_129_to_240(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];
    for i in (0..128).step_by(32) {
        acc = mix32(acc, input, i, i + 16, i, seed);
    }
    acc = [avalanche(acc[0]), avalanche(acc[1])];
    for index in 4..(len / 32) {
        let offset = index * 32;
        acc = mix32(acc, input, offset, offset + 16, 3 + (index - 4) * 32, seed);
    }
    acc = mix32(acc, input, len - 16, len - 32, 103, 0u64.wrapping_sub(seed));
    final128(acc, len, seed)
}
