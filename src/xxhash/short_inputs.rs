//! Apply XXH3 formulas to inputs that contain at most 240 bytes.

use super::primitives::*;

pub(super) fn xxh3_64_len_0_to_16(input: &[u8], seed: u64) -> u64 {
    let len = input.len();

    if len == 0 {
        return xxh64_avalanche(seed ^ (read_u64_le(&SECRET, 56) ^ read_u64_le(&SECRET, 64)));
    }

    if len <= 3 {
        let combined = (input[0] as u32) << 16
            | (input[len >> 1] as u32) << 24
            | input[len - 1] as u32
            | (len as u32) << 8;
        return xxh64_avalanche(
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

    xxh3_avalanche(
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

    xxh3_avalanche(acc)
}

pub(super) fn xxh3_64_len_129_to_240(input: &[u8], seed: u64) -> u64 {
    let len = input.len();
    let mut acc = (len as u64).wrapping_mul(P64_1);

    for i in 0..8 {
        acc = acc.wrapping_add(mix16(input, 16 * i, &SECRET, 16 * i, seed));
    }

    acc = xxh3_avalanche(acc);

    for i in 8..(len / 16) {
        acc = acc.wrapping_add(mix16(input, 16 * i, &SECRET, 3 + 16 * (i - 8), seed));
    }

    xxh3_avalanche(acc.wrapping_add(mix16(input, len - 16, &SECRET, 119, seed)))
}

#[inline(always)]
pub(super) fn xxh3_128_len_32(input: &[u8], seed: u64) -> [u64; 2] {
    final128(
        mix32([32_u64.wrapping_mul(P64_1), 0], input, 0, 16, 0, seed),
        32,
        seed,
    )
}

#[inline(always)]
pub(super) fn xxh3_128_len_64(input: &[u8], seed: u64) -> [u64; 2] {
    let acc = [64_u64.wrapping_mul(P64_1), 0];
    let acc = mix32(acc, input, 16, 32, 32, seed);

    final128(mix32(acc, input, 0, 48, 0, seed), 64, seed)
}

#[inline(always)]
pub(super) fn xxh3_128_len_128(input: &[u8], seed: u64) -> [u64; 2] {
    let acc = [128_u64.wrapping_mul(P64_1), 0];
    let acc = mix32(acc, input, 48, 64, 96, seed);
    let acc = mix32(acc, input, 32, 80, 64, seed);
    let acc = mix32(acc, input, 16, 96, 32, seed);

    final128(mix32(acc, input, 0, 112, 0, seed), 128, seed)
}

pub(super) fn xxh3_128_len_0_to_16(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();

    if len == 0 {
        return [
            xxh64_avalanche(seed ^ (read_u64_le(&SECRET, 64) ^ read_u64_le(&SECRET, 72))),
            xxh64_avalanche(seed ^ (read_u64_le(&SECRET, 80) ^ read_u64_le(&SECRET, 88))),
        ];
    }

    if len <= 3 {
        let c = (input[0] as u32) << 16
            | (input[len >> 1] as u32) << 24
            | input[len - 1] as u32
            | (len as u32) << 8;
        let h = c.swap_bytes().rotate_left(13);

        return [
            xxh64_avalanche(
                c as u64
                    ^ ((read_u32_le(&SECRET, 0) ^ read_u32_le(&SECRET, 4)) as u64)
                        .wrapping_add(seed),
            ),
            xxh64_avalanche(
                h as u64
                    ^ ((read_u32_le(&SECRET, 8) ^ read_u32_le(&SECRET, 12)) as u64)
                        .wrapping_sub(seed),
            ),
        ];
    }

    if len <= 8 {
        let seed = seed ^ ((seed as u32).swap_bytes() as u64) << 32;
        let input_word = read_u32_le(input, 0) as u64 | (read_u32_le(input, len - 4) as u64) << 32;
        let mixed =
            input_word ^ (read_u64_le(&SECRET, 16) ^ read_u64_le(&SECRET, 24)).wrapping_add(seed);
        let product = (mixed as u128) * (P64_1.wrapping_add((len as u64) << 2) as u128);
        let mut lo = product as u64;
        let mut hi = (product >> 64) as u64;
        hi = hi.wrapping_add(lo << 1);
        lo ^= hi >> 3;
        lo ^= lo >> 35;
        lo = lo.wrapping_mul(MX2);
        lo ^= lo >> 28;
        return [lo, xxh3_avalanche(hi)];
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
        xxh3_avalanche(product as u64),
        xxh3_avalanche(((product >> 64) as u64).wrapping_add(high.wrapping_mul(P64_2))),
    ]
}

#[inline(always)]
unsafe fn mix32_ptr(
    acc: &mut [u64; 2],
    first: *const u8,
    second: *const u8,
    secret: *const u8,
    seed: u64,
) {
    acc[0] = acc[0].wrapping_add(unsafe { mix16_ptr(first, secret, seed) });
    acc[0] ^= u64::from_le(unsafe { second.cast::<u64>().read_unaligned() }).wrapping_add(
        u64::from_le(unsafe { second.add(8).cast::<u64>().read_unaligned() }),
    );
    acc[1] = acc[1].wrapping_add(unsafe { mix16_ptr(second, secret.add(16), seed) });
    acc[1] ^= u64::from_le(unsafe { first.cast::<u64>().read_unaligned() }).wrapping_add(
        u64::from_le(unsafe { first.add(8).cast::<u64>().read_unaligned() }),
    );
}

#[inline(always)]
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
        xxh3_avalanche(acc[0].wrapping_add(acc[1])),
        0u64.wrapping_sub(xxh3_avalanche(
            acc[0]
                .wrapping_mul(P64_1)
                .wrapping_add(acc[1].wrapping_mul(P64_4))
                .wrapping_add((len as u64).wrapping_sub(seed).wrapping_mul(P64_2)),
        )),
    ]
}

#[inline(always)]
pub(super) fn xxh3_128_len_17_to_128(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];

    for i in (0..=((len - 1) / 32)).rev() {
        acc = mix32(acc, input, i * 16, len - 16 * (i + 1), i * 32, seed);
    }

    final128(acc, len, seed)
}

#[inline(never)]
pub(super) fn xxh3_128_len_129_to_240(input: &[u8], seed: u64) -> [u64; 2] {
    let len = input.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];
    let data = input.as_ptr();
    let secret = SECRET.as_ptr();
    assert!((129..=240).contains(&len));

    for i in (0..128).step_by(32) {
        unsafe { mix32_ptr(&mut acc, data.add(i), data.add(i + 16), secret.add(i), seed) };
    }

    acc = [xxh3_avalanche(acc[0]), xxh3_avalanche(acc[1])];

    for index in 4..(len / 32) {
        let offset = index * 32;
        unsafe {
            mix32_ptr(
                &mut acc,
                data.add(offset),
                data.add(offset + 16),
                secret.add(3 + (index - 4) * 32),
                seed,
            )
        };
    }

    unsafe {
        mix32_ptr(
            &mut acc,
            data.add(len - 16),
            data.add(len - 32),
            secret.add(103),
            0u64.wrapping_sub(seed),
        )
    };

    final128(acc, len, seed)
}
