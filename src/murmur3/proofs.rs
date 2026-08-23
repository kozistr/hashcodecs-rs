use super::primitives::{read_partial_u64_le, read_u16_le, read_u32_le, read_u64_le};
use super::x64_128::mix_x64_128_body_scalar;
use super::x86_32::mix_x86_32_body_scalar;
use super::x86_128::mix_x86_128_body_scalar;

#[kani::proof]
fn little_endian_loads_stay_within_the_slice() {
    let bytes: [u8; 16] = kani::any();
    let offset16: usize = kani::any();
    let offset32: usize = kani::any();
    let offset64: usize = kani::any();
    kani::assume(offset16 <= bytes.len() - 2);
    kani::assume(offset32 <= bytes.len() - 4);
    kani::assume(offset64 <= bytes.len() - 8);

    assert_eq!(
        read_u16_le(&bytes, offset16),
        u16::from_le_bytes([bytes[offset16], bytes[offset16 + 1]])
    );
    assert_eq!(
        read_u32_le(&bytes, offset32),
        u32::from_le_bytes([
            bytes[offset32],
            bytes[offset32 + 1],
            bytes[offset32 + 2],
            bytes[offset32 + 3],
        ])
    );
    assert_eq!(
        read_u64_le(&bytes, offset64),
        u64::from_le_bytes([
            bytes[offset64],
            bytes[offset64 + 1],
            bytes[offset64 + 2],
            bytes[offset64 + 3],
            bytes[offset64 + 4],
            bytes[offset64 + 5],
            bytes[offset64 + 6],
            bytes[offset64 + 7],
        ])
    );
}

#[kani::proof]
#[kani::unwind(9)]
fn scalar_block_loops_and_partial_loads_stay_in_bounds() {
    let input: [u8; 32] = kani::any();
    let partial_length: usize = kani::any();
    kani::assume(partial_length <= 8);
    let _ = read_partial_u64_le(&input[..partial_length]);

    let mut hash32: u32 = kani::any();
    mix_x86_32_body_scalar(&input, &mut hash32);
    let mut hashes_x86: [u32; 4] = kani::any();
    mix_x86_128_body_scalar(&input, &mut hashes_x86);
    let mut hashes_x64: [u64; 2] = kani::any();
    mix_x64_128_body_scalar(&input, &mut hashes_x64);
}
