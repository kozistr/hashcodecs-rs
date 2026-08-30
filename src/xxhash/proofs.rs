use super::long_inputs::{LongInput, build_long_input_schedule};
use super::primitives::{SECRET, u32le, u64le};

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
    kani::assume(length > 240 && length <= 2048);
    let bytes = [0_u8; 2048];
    let input = LongInput::new(&bytes[..length]).unwrap();
    let schedule = build_long_input_schedule(input);

    let block: usize = kani::any();
    let block_stripe: usize = kani::any();
    kani::assume(block_stripe < 16);
    if block < schedule.full_blocks() {
        let block_offset = block * 1024 + block_stripe * 64;
        assert!(block_offset <= length - 64);
        if block + 2 <= schedule.full_blocks() {
            assert!((block + 2) * 1024 < length);
        }
    }

    let tail_stripe: usize = kani::any();
    if tail_stripe < schedule.tail_stripes() {
        let tail_offset = schedule.tail_offset() + tail_stripe * 64;
        assert!(tail_offset <= length - 64);
        assert!(tail_stripe * 8 <= SECRET.len() - 64);
    }
    assert!(schedule.last_offset() <= length - 64);

    assert!(block_stripe * 8 <= SECRET.len() - 64);
    assert!(121 <= SECRET.len() - 64);
    assert!(128 <= SECRET.len() - 64);
}
