#[inline(always)]
pub(super) fn fmix32(mut hash: u32) -> u32 {
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ (hash >> 16)
}

#[inline(always)]
pub(super) fn fmix64(mut hash: u64) -> u64 {
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

#[inline(always)]
pub(super) fn read_u16_le(input: &[u8], offset: usize) -> u16 {
    debug_assert!(offset + 2 <= input.len());
    u16::from_le_bytes(
        input[offset..offset + 2]
            .try_into()
            .expect("two-byte load exceeds input"),
    )
}

#[inline(always)]
pub(super) fn read_u32_le(input: &[u8], offset: usize) -> u32 {
    debug_assert!(offset + 4 <= input.len());
    // The caller's loop checks the bounds. The compiler still emits one unaligned load for this conversion.
    u32::from_le_bytes(
        input[offset..offset + 4]
            .try_into()
            .expect("four-byte load exceeds input"),
    )
}

#[inline(always)]
pub(super) fn read_u64_le(input: &[u8], offset: usize) -> u64 {
    debug_assert!(offset + 8 <= input.len());
    // The `read_u32_le` comment describes the same bound check. `from_le_bytes` also normalizes byte order.
    u64::from_le_bytes(
        input[offset..offset + 8]
            .try_into()
            .expect("eight-byte load exceeds input"),
    )
}

#[inline(always)]
pub(super) fn read_partial_u64_le(input: &[u8]) -> u64 {
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
