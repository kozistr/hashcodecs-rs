pub(super) const P32_1: u64 = 2_654_435_761;
pub(super) const P32_2: u64 = 2_246_822_519;
pub(super) const P32_3: u64 = 3_266_489_917;
pub(super) const P64_1: u64 = 11_400_714_785_074_694_791;
pub(super) const P64_2: u64 = 14_029_467_366_897_019_727;
pub(super) const P64_3: u64 = 1_609_587_929_392_839_161;
pub(super) const P64_4: u64 = 9_650_029_242_287_828_579;
pub(super) const P64_5: u64 = 2_870_177_450_012_600_261;
pub(super) const MX1: u64 = 0x1656_6791_9e37_79f9;
pub(super) const MX2: u64 = 0x9fb2_1c65_1e98_df25;

pub(super) const SECRET: [u8; 192] = [
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
pub(super) fn read_u32_le(s: &[u8], o: usize) -> u32 {
    // Length-class dispatch checks this range before optimized callers run.
    // Checked indexing keeps this helper safe if a future caller supplies an invalid range.
    u32::from_le_bytes(
        s[o..o + 4]
            .try_into()
            .expect("four-byte load exceeds input"),
    )
}

#[inline(always)]
pub(super) fn read_u64_le(s: &[u8], o: usize) -> u64 {
    // The `read_u32_le` comment describes the same invariant for a four-byte word.
    u64::from_le_bytes(
        s[o..o + 8]
            .try_into()
            .expect("eight-byte load exceeds input"),
    )
}

#[inline(always)]
pub(super) fn mul_fold(a: u64, b: u64) -> u64 {
    let p = (a as u128) * (b as u128);
    p as u64 ^ (p >> 64) as u64
}

#[inline(always)]
pub(super) fn avalanche(mut h: u64) -> u64 {
    h ^= h >> 37;
    h = h.wrapping_mul(MX1);
    h ^ (h >> 32)
}

#[inline(always)]
pub(super) fn avalanche64(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(P64_2);
    h ^= h >> 29;
    h = h.wrapping_mul(P64_3);
    h ^ (h >> 32)
}

#[inline(always)]
pub(super) fn rrmxmx(mut h: u64, len: usize) -> u64 {
    h ^= h.rotate_left(49) ^ h.rotate_left(24);
    h = h.wrapping_mul(MX2);
    h ^= (h >> 35).wrapping_add(len as u64);
    h = h.wrapping_mul(MX2);
    h ^ (h >> 28)
}

#[inline(always)]
pub(super) fn mix16(
    data: &[u8],
    data_offset: usize,
    secret: &[u8],
    secret_offset: usize,
    seed: u64,
) -> u64 {
    assert!(
        data_offset
            .checked_add(16)
            .is_some_and(|end| end <= data.len())
    );
    assert!(
        secret_offset
            .checked_add(16)
            .is_some_and(|end| end <= secret.len())
    );

    unsafe {
        mix16_ptr(
            data.as_ptr().add(data_offset),
            secret.as_ptr().add(secret_offset),
            seed,
        )
    }
}

#[inline(always)]
unsafe fn mix16_ptr(data: *const u8, secret: *const u8, seed: u64) -> u64 {
    let data_lo = u64::from_le(unsafe { data.cast::<u64>().read_unaligned() });
    let data_hi = u64::from_le(unsafe { data.add(8).cast::<u64>().read_unaligned() });

    let secret_lo = u64::from_le(unsafe { secret.cast::<u64>().read_unaligned() });
    let secret_hi = u64::from_le(unsafe { secret.add(8).cast::<u64>().read_unaligned() });

    let lo = data_lo ^ secret_lo.wrapping_add(seed);
    let hi = data_hi ^ secret_hi.wrapping_sub(seed);

    mul_fold(lo, hi)
}
