use super::{LongInput, Secret, initial_accumulator, long_schedule};
use crate::xxhash::primitives::{P32_1, u64le};

#[inline(always)]
fn accumulate_stripe(
    acc: &mut [u64; 8],
    data: &[u8],
    secret: &Secret,
    data_offset: usize,
    secret_offset: usize,
) {
    let secret = secret.as_bytes();
    for lane in 0..8 {
        let value = u64le(data, data_offset + lane * 8);
        let keyed = value ^ u64le(secret, secret_offset + lane * 8);
        acc[lane ^ 1] = acc[lane ^ 1].wrapping_add(value);
        acc[lane] = acc[lane].wrapping_add((keyed as u32 as u64).wrapping_mul(keyed >> 32));
    }
}

#[inline(always)]
fn scramble(acc: &mut [u64; 8], secret: &Secret) {
    let secret = secret.as_bytes();
    for (lane, value) in acc.iter_mut().enumerate() {
        *value ^= *value >> 47;
        *value ^= u64le(secret, 128 + lane * 8);
        *value = value.wrapping_mul(P32_1);
    }
}

pub(super) fn accumulate(input: LongInput<'_>, secret: &Secret) -> [u64; 8] {
    let data = input.as_bytes();
    let schedule = long_schedule(input);
    let mut acc = initial_accumulator();
    for block in 0..schedule.full_blocks() {
        let offset = block * 1024;
        for stripe in 0..16 {
            accumulate_stripe(&mut acc, data, secret, offset + stripe * 64, stripe * 8);
        }
        scramble(&mut acc, secret);
    }
    for stripe in 0..schedule.tail_stripes() {
        accumulate_stripe(
            &mut acc,
            data,
            secret,
            schedule.tail_offset() + stripe * 64,
            stripe * 8,
        );
    }
    accumulate_stripe(&mut acc, data, secret, schedule.last_offset(), 121);
    acc
}
