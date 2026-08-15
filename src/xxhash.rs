//! In-tree XXH3 implementation.
//!
//! This module follows the public xxHash algorithm specification.  The scalar
//! core is the portability baseline; architecture-specific accumulation is
//! deliberately kept behind dispatch points so unsupported CPUs never execute
//! instructions they cannot run.

#[cfg(all(not(any(coverage, kani, miri)), target_arch = "aarch64"))]
mod aarch64;
#[cfg(all(
    not(any(coverage, kani, miri)),
    any(target_arch = "x86", target_arch = "x86_64")
))]
mod x86;

const P32_1: u64 = 2_654_435_761;
const P32_2: u64 = 2_246_822_519;
const P32_3: u64 = 3_266_489_917;
const P64_1: u64 = 11_400_714_785_074_694_791;
const P64_2: u64 = 14_029_467_366_897_019_727;
const P64_3: u64 = 1_609_587_929_392_839_161;
const P64_4: u64 = 9_650_029_242_287_828_579;
const P64_5: u64 = 2_870_177_450_012_600_261;
const MX1: u64 = 0x1656_6791_9e37_79f9;
const MX2: u64 = 0x9fb2_1c65_1e98_df25;

const SECRET: [u8; 192] = [
    0xb8, 0xfe, 0x6c, 0x39, 0x23, 0xa4, 0x4b, 0xbe, 0x7c, 0x01, 0x81, 0x2c, 0xf7, 0x21, 0xad, 0x1c,
    0xde, 0xd4, 0x6d, 0xe9, 0x83, 0x90, 0x97, 0xdb, 0x72, 0x40, 0xa4, 0xa4, 0xb7, 0xb3, 0x67, 0x1f,
    0xcb, 0x79, 0xe6, 0x4e, 0xcc, 0xc0, 0xe5, 0x78, 0x82, 0x5a, 0xd0, 0x7d, 0xcc, 0xff, 0x72, 0x21,
    0xb8, 0x08, 0x46, 0x74, 0xf7, 0x43, 0x24, 0x8e, 0xe0, 0x35, 0x90, 0xe6, 0x81, 0x3a, 0x26, 0x4c,
    0x3c, 0x28, 0x52, 0xbb, 0x91, 0xc3, 0x00, 0xcb, 0x88, 0xd0, 0x65, 0x8b, 0x1b, 0x53, 0x2e, 0xa3,
    0x71, 0x64, 0x48, 0x97, 0xa2, 0x0d, 0xf9, 0x4e, 0x38, 0x19, 0xef, 0x46, 0xa9, 0xde, 0xac, 0xd8,
    0xa8, 0xfa, 0x76, 0x3f, 0xe3, 0x9c, 0x34, 0x3f, 0xf9, 0xdc, 0xbb, 0xc7, 0xc7, 0x0b, 0x4f, 0x1d,
    0x8a, 0x51, 0xe0, 0x4b, 0xcd, 0xb4, 0x59, 0x31, 0xc8, 0x9f, 0x7e, 0xc9, 0xd9, 0x78, 0x73, 0x64,
    0xea, 0xc5, 0xac, 0x83, 0x34, 0xd3, 0xeb, 0xc3, 0xc5, 0x81, 0xa0, 0xff, 0xfa, 0x13, 0x63, 0xeb,
    0x17, 0x0d, 0xdd, 0x51, 0xb7, 0xf0, 0xda, 0x49, 0xd3, 0x16, 0x55, 0x26, 0x29, 0xd4, 0x68, 0x9e,
    0x2b, 0x16, 0xbe, 0x58, 0x7d, 0x47, 0xa1, 0xfc, 0x8f, 0xf8, 0xb8, 0xd1, 0x7a, 0xd0, 0x31, 0xce,
    0x45, 0xcb, 0x3a, 0x8f, 0x95, 0x16, 0x04, 0x28, 0xaf, 0xd7, 0xfb, 0xca, 0xbb, 0x4b, 0x40, 0x7e,
];

#[inline(always)]
fn u32le(s: &[u8], o: usize) -> u32 {
    // SAFETY: XXH3's length-class dispatch guarantees a complete word at each
    // offset. Kani proves the primitive for every valid offset, and Miri checks
    // every algorithm boundary used by its callers.
    unsafe { u32::from_le(s.as_ptr().add(o).cast::<u32>().read_unaligned()) }
}
#[inline(always)]
fn u64le(s: &[u8], o: usize) -> u64 {
    // SAFETY: See `u32le`; this is the same invariant for an eight-byte word.
    unsafe { u64::from_le(s.as_ptr().add(o).cast::<u64>().read_unaligned()) }
}
#[inline(always)]
fn mulfold(a: u64, b: u64) -> u64 {
    let p = (a as u128) * (b as u128);
    p as u64 ^ (p >> 64) as u64
}
#[inline(always)]
fn avalanche(mut h: u64) -> u64 {
    h ^= h >> 37;
    h = h.wrapping_mul(MX1);
    h ^ (h >> 32)
}
#[inline(always)]
fn avalanche64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(P64_2);
    h ^= h >> 29;
    h = h.wrapping_mul(P64_3);
    h ^ (h >> 32)
}
#[inline(always)]
fn rrmxmx(mut h: u64, len: usize) -> u64 {
    h ^= h.rotate_left(49) ^ h.rotate_left(24);
    h = h.wrapping_mul(MX2);
    h ^= (h >> 35).wrapping_add(len as u64);
    h = h.wrapping_mul(MX2);
    h ^ (h >> 28)
}
#[inline(always)]
fn mix16(data: &[u8], doff: usize, secret: &[u8], soff: usize, seed: u64) -> u64 {
    let lo = u64le(data, doff) ^ u64le(secret, soff).wrapping_add(seed);
    let hi = u64le(data, doff + 8) ^ u64le(secret, soff + 8).wrapping_sub(seed);
    mulfold(lo, hi)
}

/// Computes the canonical XXH3 64-bit digest.
#[inline]
pub fn xxh3_64(input: &[u8], seed: u64) -> u64 {
    match input.len() {
        0..=16 => xxh3_64_small(input, seed),
        17..=128 => xxh3_64_medium(input, seed),
        129..=240 => xxh3_64_midsize(input, seed),
        _ => xxh3_64_long(input, seed),
    }
}

#[inline]
fn xxh3_64_with_long_secret(input: &[u8], seed: u64, long_secret: &[u8]) -> u64 {
    match input.len() {
        0..=16 => xxh3_64_small(input, seed),
        17..=128 => xxh3_64_medium(input, seed),
        129..=240 => xxh3_64_midsize(input, seed),
        _ => {
            let acc = long_accumulate(input, long_secret);
            merge(
                &acc,
                &long_secret[11..],
                (input.len() as u64).wrapping_mul(P64_1),
            )
        }
    }
}

fn xxh3_64_small(data: &[u8], seed: u64) -> u64 {
    let len = data.len();
    if len == 0 {
        return avalanche64(seed ^ (u64le(&SECRET, 56) ^ u64le(&SECRET, 64)));
    }
    if len <= 3 {
        let combined = (data[0] as u32) << 16
            | (data[len >> 1] as u32) << 24
            | data[len - 1] as u32
            | (len as u32) << 8;
        return avalanche64(
            (combined as u64) ^ ((u32le(&SECRET, 0) ^ u32le(&SECRET, 4)) as u64).wrapping_add(seed),
        );
    }
    if len <= 8 {
        let seed = seed ^ ((seed as u32).swap_bytes() as u64) << 32;
        let input = u32le(data, len - 4) as u64 | (u32le(data, 0) as u64) << 32;
        return rrmxmx(
            input ^ (u64le(&SECRET, 8) ^ u64le(&SECRET, 16)).wrapping_sub(seed),
            len,
        );
    }
    let lo = u64le(data, 0) ^ (u64le(&SECRET, 24) ^ u64le(&SECRET, 32)).wrapping_add(seed);
    let hi = u64le(data, len - 8) ^ (u64le(&SECRET, 40) ^ u64le(&SECRET, 48)).wrapping_sub(seed);
    avalanche(
        (len as u64)
            .wrapping_add(lo.swap_bytes())
            .wrapping_add(hi)
            .wrapping_add(mulfold(lo, hi)),
    )
}

fn xxh3_64_medium(data: &[u8], seed: u64) -> u64 {
    let len = data.len();
    let mut acc = (len as u64).wrapping_mul(P64_1);
    if len > 32 {
        if len > 64 {
            if len > 96 {
                acc = acc.wrapping_add(mix16(data, 48, &SECRET, 96, seed));
                acc = acc.wrapping_add(mix16(data, len - 64, &SECRET, 112, seed));
            }
            acc = acc.wrapping_add(mix16(data, 32, &SECRET, 64, seed));
            acc = acc.wrapping_add(mix16(data, len - 48, &SECRET, 80, seed));
        }
        acc = acc.wrapping_add(mix16(data, 16, &SECRET, 32, seed));
        acc = acc.wrapping_add(mix16(data, len - 32, &SECRET, 48, seed));
    }
    acc = acc.wrapping_add(mix16(data, 0, &SECRET, 0, seed));
    acc = acc.wrapping_add(mix16(data, len - 16, &SECRET, 16, seed));
    avalanche(acc)
}

fn xxh3_64_midsize(data: &[u8], seed: u64) -> u64 {
    let len = data.len();
    let mut acc = (len as u64).wrapping_mul(P64_1);
    for i in 0..8 {
        acc = acc.wrapping_add(mix16(data, 16 * i, &SECRET, 16 * i, seed));
    }
    acc = avalanche(acc);
    for i in 8..(len / 16) {
        acc = acc.wrapping_add(mix16(data, 16 * i, &SECRET, 3 + 16 * (i - 8), seed));
    }
    avalanche(acc.wrapping_add(mix16(data, len - 16, &SECRET, 119, seed)))
}

fn init_secret_scalar(seed: u64) -> [u8; 192] {
    let mut secret = SECRET;
    for offset in (0..192).step_by(16) {
        let lo = u64le(&SECRET, offset).wrapping_add(seed);
        let hi = u64le(&SECRET, offset + 8).wrapping_sub(seed);
        secret[offset..offset + 8].copy_from_slice(&lo.to_le_bytes());
        secret[offset + 8..offset + 16].copy_from_slice(&hi.to_le_bytes());
    }
    secret
}

#[inline]
fn init_secret(seed: u64) -> [u8; 192] {
    #[cfg(all(
        not(any(coverage, kani, miri)),
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    if std::is_x86_feature_detected!("avx2") {
        return unsafe { x86::init_secret_avx2(seed) };
    }
    init_secret_scalar(seed)
}
#[inline(always)]
fn accumulate_scalar(acc: &mut [u64; 8], data: &[u8], secret: &[u8], offset: usize) {
    for lane in 0..8 {
        let value = u64le(data, offset + lane * 8);
        let keyed = value ^ u64le(secret, lane * 8);
        acc[lane ^ 1] = acc[lane ^ 1].wrapping_add(value);
        acc[lane] = acc[lane].wrapping_add((keyed as u32 as u64).wrapping_mul(keyed >> 32));
    }
}
#[inline(always)]
fn scramble_scalar(acc: &mut [u64; 8], secret: &[u8]) {
    for (lane, value) in acc.iter_mut().enumerate() {
        *value ^= *value >> 47;
        *value ^= u64le(secret, lane * 8);
        *value = value.wrapping_mul(P32_1);
    }
}
fn merge(acc: &[u64; 8], secret: &[u8], start: u64) -> u64 {
    let mut result = start;
    for lane in 0..4 {
        result = result.wrapping_add(mulfold(
            acc[lane * 2] ^ u64le(secret, lane * 16),
            acc[lane * 2 + 1] ^ u64le(secret, lane * 16 + 8),
        ));
    }
    avalanche(result)
}
#[inline]
fn initial_accumulator() -> [u64; 8] {
    [P32_3, P64_1, P64_2, P64_3, P64_4, P32_2, P64_5, P32_1]
}

#[derive(Clone, Copy)]
struct LongSchedule {
    full_blocks: usize,
    tail_offset: usize,
    tail_stripes: usize,
    last_offset: usize,
}

#[inline]
fn long_schedule(length: usize) -> LongSchedule {
    let full_blocks = (length - 1) / 1024;
    let tail_offset = full_blocks * 1024;
    LongSchedule {
        full_blocks,
        tail_offset,
        tail_stripes: (length - tail_offset - 1) / 64,
        last_offset: length - 64,
    }
}

fn long_accumulate_scalar(data: &[u8], secret: &[u8]) -> [u64; 8] {
    let schedule = long_schedule(data.len());
    let mut acc = initial_accumulator();
    for block in 0..schedule.full_blocks {
        let offset = block * 1024;
        for stripe in 0..16 {
            accumulate_scalar(&mut acc, data, &secret[stripe * 8..], offset + stripe * 64);
        }
        scramble_scalar(&mut acc, &secret[128..]);
    }
    for stripe in 0..schedule.tail_stripes {
        accumulate_scalar(
            &mut acc,
            data,
            &secret[stripe * 8..],
            schedule.tail_offset + stripe * 64,
        );
    }
    accumulate_scalar(&mut acc, data, &secret[121..], schedule.last_offset);
    acc
}

#[inline]
fn long_accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    #[cfg(all(
        not(any(coverage, kani, miri)),
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    {
        match x86::backend() {
            x86::Backend::Avx512 => return unsafe { x86::long_accumulate_avx512(data, secret) },
            x86::Backend::Avx2 => return unsafe { x86::long_accumulate_avx2(data, secret) },
            x86::Backend::Sse41 => return unsafe { x86::long_accumulate_sse41(data, secret) },
            x86::Backend::Ssse3 => return unsafe { x86::long_accumulate_ssse3(data, secret) },
            x86::Backend::Scalar => {}
        }
    }
    #[cfg(all(not(any(coverage, kani, miri)), target_arch = "aarch64"))]
    if std::arch::is_aarch64_feature_detected!("neon") {
        return unsafe { aarch64::long_accumulate_neon(data, secret) };
    }
    long_accumulate_scalar(data, secret)
}
fn xxh3_64_long(data: &[u8], seed: u64) -> u64 {
    let secret = (seed != 0).then(|| init_secret(seed));
    let secret = secret.as_ref().unwrap_or(&SECRET);
    let acc = long_accumulate(data, secret);
    merge(&acc, &secret[11..], (data.len() as u64).wrapping_mul(P64_1))
}

/// Computes the canonical XXH3 128-bit digest as `[low64, high64]`.
#[inline]
pub fn xxh3_128(input: &[u8], seed: u64) -> [u64; 2] {
    match input.len() {
        0..=16 => xxh3_128_small(input, seed),
        17..=128 => xxh3_128_medium(input, seed),
        129..=240 => xxh3_128_midsize(input, seed),
        _ => xxh3_128_long(input, seed),
    }
}

#[inline]
fn xxh3_128_with_long_secret(input: &[u8], seed: u64, long_secret: &[u8]) -> [u64; 2] {
    match input.len() {
        0..=16 => xxh3_128_small(input, seed),
        17..=128 => xxh3_128_medium(input, seed),
        129..=240 => xxh3_128_midsize(input, seed),
        _ => finalize_long_128(
            input.len(),
            long_secret,
            long_accumulate(input, long_secret),
        ),
    }
}

fn xxh3_128_small(data: &[u8], seed: u64) -> [u64; 2] {
    let len = data.len();
    if len == 0 {
        return [
            avalanche64(seed ^ (u64le(&SECRET, 64) ^ u64le(&SECRET, 72))),
            avalanche64(seed ^ (u64le(&SECRET, 80) ^ u64le(&SECRET, 88))),
        ];
    }
    if len <= 3 {
        let c = (data[0] as u32) << 16
            | (data[len >> 1] as u32) << 24
            | data[len - 1] as u32
            | (len as u32) << 8;
        let h = c.swap_bytes().rotate_left(13);
        return [
            avalanche64(
                c as u64 ^ ((u32le(&SECRET, 0) ^ u32le(&SECRET, 4)) as u64).wrapping_add(seed),
            ),
            avalanche64(
                h as u64 ^ ((u32le(&SECRET, 8) ^ u32le(&SECRET, 12)) as u64).wrapping_sub(seed),
            ),
        ];
    }
    if len <= 8 {
        let seed = seed ^ ((seed as u32).swap_bytes() as u64) << 32;
        let input = u32le(data, 0) as u64 | (u32le(data, len - 4) as u64) << 32;
        let keyed = input ^ (u64le(&SECRET, 16) ^ u64le(&SECRET, 24)).wrapping_add(seed);
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
    let lo = u64le(data, 0);
    let mut hi = u64le(data, len - 8);
    let product = ((lo ^ hi ^ (u64le(&SECRET, 32) ^ u64le(&SECRET, 40)).wrapping_sub(seed))
        as u128)
        * P64_1 as u128;
    let mut low = (product as u64).wrapping_add(((len - 1) as u64) << 54);
    let mut high = (product >> 64) as u64;
    hi ^= (u64le(&SECRET, 48) ^ u64le(&SECRET, 56)).wrapping_add(seed);
    high = high.wrapping_add(hi.wrapping_add((hi as u32 as u64).wrapping_mul(P32_2 - 1)));
    low ^= high.swap_bytes();
    let product = (low as u128) * P64_2 as u128;
    [
        avalanche(product as u64),
        avalanche(((product >> 64) as u64).wrapping_add(high.wrapping_mul(P64_2))),
    ]
}

fn mix32(
    mut acc: [u64; 2],
    data: &[u8],
    first: usize,
    second: usize,
    secret: usize,
    seed: u64,
) -> [u64; 2] {
    acc[0] = acc[0].wrapping_add(mix16(data, first, &SECRET, secret, seed));
    acc[0] ^= u64le(data, second).wrapping_add(u64le(data, second + 8));
    acc[1] = acc[1].wrapping_add(mix16(data, second, &SECRET, secret + 16, seed));
    acc[1] ^= u64le(data, first).wrapping_add(u64le(data, first + 8));
    acc
}
fn final128(acc: [u64; 2], len: usize, seed: u64) -> [u64; 2] {
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
fn xxh3_128_medium(data: &[u8], seed: u64) -> [u64; 2] {
    let len = data.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];
    for i in (0..=((len - 1) / 32)).rev() {
        acc = mix32(acc, data, i * 16, len - 16 * (i + 1), i * 32, seed);
    }
    final128(acc, len, seed)
}
fn xxh3_128_midsize(data: &[u8], seed: u64) -> [u64; 2] {
    let len = data.len();
    let mut acc = [(len as u64).wrapping_mul(P64_1), 0];
    for i in (0..128).step_by(32) {
        acc = mix32(acc, data, i, i + 16, i, seed);
    }
    acc = [avalanche(acc[0]), avalanche(acc[1])];
    for index in 4..(len / 32) {
        let offset = index * 32;
        acc = mix32(acc, data, offset, offset + 16, 3 + (index - 4) * 32, seed);
    }
    acc = mix32(acc, data, len - 16, len - 32, 103, 0u64.wrapping_sub(seed));
    final128(acc, len, seed)
}
fn xxh3_128_long(data: &[u8], seed: u64) -> [u64; 2] {
    let secret = (seed != 0).then(|| init_secret(seed));
    let secret = secret.as_ref().unwrap_or(&SECRET);
    finalize_long_128(data.len(), secret, long_accumulate(data, secret))
}

#[inline]
fn finalize_long_128(length: usize, secret: &[u8], acc: [u64; 8]) -> [u64; 2] {
    [
        merge(&acc, &secret[11..], (length as u64).wrapping_mul(P64_1)),
        merge(&acc, &secret[117..], !(length as u64).wrapping_mul(P64_2)),
    ]
}

#[inline]
fn batch4_long_accumulators(chunk: &[&[u8]], secret: &[u8]) -> Option<[[u64; 8]; 4]> {
    let _ = (chunk, secret);
    #[cfg(coverage)]
    if chunk[0].len() > 240 && chunk.iter().all(|input| input.len() == chunk[0].len()) {
        return Some([
            long_accumulate_scalar(chunk[0], secret),
            long_accumulate_scalar(chunk[1], secret),
            long_accumulate_scalar(chunk[2], secret),
            long_accumulate_scalar(chunk[3], secret),
        ]);
    }
    #[cfg(all(
        not(any(coverage, kani, miri)),
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    if chunk[0].len() > 240
        && chunk.iter().all(|input| input.len() == chunk[0].len())
        && std::is_x86_feature_detected!("avx2")
        && matches!(x86::backend(), x86::Backend::Avx2 | x86::Backend::Avx512)
    {
        let values = [chunk[0], chunk[1], chunk[2], chunk[3]];
        return Some(unsafe { x86::long_accumulate_batch4_avx2(values, secret) });
    }
    None
}

/// Hashes a batch without input copies. The caller owns output allocation.
/// Equal-size long inputs are processed four-way on AVX2 to overlap independent
/// multiply and load latency.
#[inline]
pub fn xxh3_64_batch(inputs: &[&[u8]], seed: u64) -> Vec<u64> {
    let owned_secret = (seed != 0).then(|| init_secret(seed));
    let secret = owned_secret.as_ref().unwrap_or(&SECRET);
    let mut output = Vec::with_capacity(inputs.len());
    let mut chunks = inputs.chunks_exact(4);
    for chunk in &mut chunks {
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
        chunks
            .remainder()
            .iter()
            .map(|input| xxh3_64_with_long_secret(input, seed, secret)),
    );
    output
}
/// Hashes a batch without input copies. Words are ordered `[low64, high64]`.
/// Equal-size long inputs are processed four-way on AVX2 to overlap independent
/// multiply and load latency.
#[inline]
pub fn xxh3_128_batch(inputs: &[&[u8]], seed: u64) -> Vec<[u64; 2]> {
    let owned_secret = (seed != 0).then(|| init_secret(seed));
    let secret = owned_secret.as_ref().unwrap_or(&SECRET);
    let mut output = Vec::with_capacity(inputs.len());
    let mut chunks = inputs.chunks_exact(4);
    for chunk in &mut chunks {
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
        chunks
            .remainder()
            .iter()
            .map(|input| xxh3_128_with_long_secret(input, seed, secret)),
    );
    output
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn little_endian_loads_stay_within_the_slice() {
        let bytes: [u8; 16] = kani::any();
        let offset32: usize = kani::any();
        let offset64: usize = kani::any();
        kani::assume(offset32 <= bytes.len() - 4);
        kani::assume(offset64 <= bytes.len() - 8);

        let expected32 = u32::from_le_bytes([
            bytes[offset32],
            bytes[offset32 + 1],
            bytes[offset32 + 2],
            bytes[offset32 + 3],
        ]);
        let expected64 = u64::from_le_bytes([
            bytes[offset64],
            bytes[offset64 + 1],
            bytes[offset64 + 2],
            bytes[offset64 + 3],
            bytes[offset64 + 4],
            bytes[offset64 + 5],
            bytes[offset64 + 6],
            bytes[offset64 + 7],
        ]);
        assert_eq!(u32le(&bytes, offset32), expected32);
        assert_eq!(u64le(&bytes, offset64), expected64);
    }

    #[kani::proof]
    fn long_schedule_keeps_vector_loads_in_bounds() {
        let length: usize = kani::any();
        kani::assume(length > 240);
        let schedule = long_schedule(length);

        let block: usize = kani::any();
        let block_stripe: usize = kani::any();
        kani::assume(block_stripe < 16);
        if block < schedule.full_blocks {
            let block_offset = block * 1024 + block_stripe * 64;
            assert!(block_offset <= length - 64);
            if block + 2 <= schedule.full_blocks {
                assert!((block + 2) * 1024 < length);
            }
        }

        let tail_stripe: usize = kani::any();
        if tail_stripe < schedule.tail_stripes {
            let tail_offset = schedule.tail_offset + tail_stripe * 64;
            assert!(tail_offset <= length - 64);
            assert!(tail_stripe * 8 <= SECRET.len() - 64);
        }
        assert!(schedule.last_offset <= length - 64);

        assert!(block_stripe * 8 <= SECRET.len() - 64);
        assert!(121 <= SECRET.len() - 64);
        assert!(128 <= SECRET.len() - 64);
    }
}

#[cfg(all(test, miri))]
mod miri_tests {
    use super::*;

    #[test]
    fn every_length_class_and_batch_are_defined() {
        const LENGTHS: &[usize] = &[
            0, 1, 3, 4, 8, 9, 16, 17, 32, 33, 64, 65, 96, 97, 128, 129, 160, 191, 224, 239, 240,
            241, 1023, 1024, 1025, 2049,
        ];
        for &length in LENGTHS {
            let input = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect::<Vec<_>>();
            for &seed in &[0, 1, u64::MAX] {
                let _ = xxh3_64(&input, seed);
                let _ = xxh3_128(&input, seed);
            }
        }

        let owned = (0..4).map(|item| vec![item; 2049]).collect::<Vec<_>>();
        let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert_eq!(
            xxh3_64_batch(&inputs, 42),
            inputs
                .iter()
                .map(|input| xxh3_64(input, 42))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            xxh3_128_batch(&inputs, 42),
            inputs
                .iter()
                .map(|input| xxh3_128(input, 42))
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod tests {
    use core::ffi::c_void;

    use super::*;

    fn c_xxh3_64(input: &[u8], seed: u64) -> u64 {
        unsafe {
            xxhash_c_sys::XXH3_64bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
        }
    }

    fn c_xxh3_128(input: &[u8], seed: u64) -> [u64; 2] {
        let hash = unsafe {
            xxhash_c_sys::XXH3_128bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
        };
        [hash.low64, hash.high64]
    }

    #[test]
    fn empty_vectors() {
        assert_eq!(xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
        assert_eq!(
            xxh3_128(b"", 0),
            [0x6001_c324_468d_497f, 0x99aa_06d3_0147_98d8]
        );
    }
    #[test]
    fn batches_match_one_shot() {
        let values: [&[u8]; 3] = [b"", b"hello", b"xxhash"];
        assert_eq!(xxh3_64_batch(&values, 42), values.map(|v| xxh3_64(v, 42)));
        assert_eq!(xxh3_128_batch(&values, 42), values.map(|v| xxh3_128(v, 42)));

        let mixed_owned = [17, 129, 241, 300].map(|length| {
            (0..length)
                .map(|index| (index as u8).wrapping_mul(19).wrapping_add(7))
                .collect::<Vec<_>>()
        });
        let mixed = mixed_owned.each_ref().map(Vec::as_slice);
        assert_eq!(xxh3_64_batch(&mixed, 42), mixed.map(|v| xxh3_64(v, 42)));
        assert_eq!(xxh3_128_batch(&mixed, 42), mixed.map(|v| xxh3_128(v, 42)));

        let owned = (0..8)
            .map(|item| {
                (0..4097)
                    .map(|index| (index as u8).wrapping_mul(31).wrapping_add(item))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert_eq!(
            xxh3_64_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| xxhash_rust::xxh3::xxh3_64_with_seed(input, 0x1234_5678))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            xxh3_128_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| {
                    let hash = xxhash_rust::xxh3::xxh3_128_with_seed(input, 0x1234_5678);
                    [hash as u64, (hash >> 64) as u64]
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn matches_reference_at_every_length_through_two_blocks() {
        let input = (0..=2048)
            .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
            .collect::<Vec<_>>();
        for length in 0..=2048 {
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
    fn matches_xxhash_reference_at_boundaries_and_large_lengths() {
        const LENGTHS: &[usize] = &[
            0, 1, 2, 3, 4, 8, 9, 16, 17, 31, 32, 33, 63, 64, 65, 96, 97, 127, 128, 129, 159, 160,
            191, 192, 239, 240, 241, 255, 256, 511, 512, 1023, 1024, 1025, 4097,
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
    fn randomized_inputs_match_the_official_c_implementation() {
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

    #[cfg(all(
        not(any(coverage, kani, miri)),
        any(target_arch = "x86", target_arch = "x86_64")
    ))]
    #[test]
    fn every_supported_x86_backend_matches_scalar() {
        let input: Vec<u8> = (0..4097)
            .map(|index| (index as u8).wrapping_mul(47).wrapping_add(91))
            .collect();
        for &seed in &[0, 1, 0xfeed_beef_cafe_babe] {
            let owned_secret = (seed != 0).then(|| init_secret(seed));
            let secret = owned_secret.as_ref().unwrap_or(&SECRET);
            let expected = long_accumulate_scalar(&input, secret);
            for backend in [
                x86::Backend::Ssse3,
                x86::Backend::Sse41,
                x86::Backend::Avx2,
                x86::Backend::Avx512,
            ] {
                if !x86::backend_supported(backend) {
                    continue;
                }
                let actual = match backend {
                    x86::Backend::Sse41 => unsafe { x86::long_accumulate_sse41(&input, secret) },
                    x86::Backend::Ssse3 => unsafe { x86::long_accumulate_ssse3(&input, secret) },
                    x86::Backend::Avx2 => unsafe { x86::long_accumulate_avx2(&input, secret) },
                    x86::Backend::Avx512 => unsafe { x86::long_accumulate_avx512(&input, secret) },
                    x86::Backend::Scalar => unreachable!(),
                };
                assert_eq!(actual, expected, "{backend:?} mismatch for seed {seed:#x}");
            }
        }
    }
}
