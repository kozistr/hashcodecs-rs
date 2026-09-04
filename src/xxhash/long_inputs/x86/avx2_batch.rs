//! Interleaved AVX2 kernels for equal-length long-input batches.

#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use super::super::{LongInput, Secret, build_long_input_schedule};
use super::avx2::{accumulate_registers, finish, initial, scramble_registers};

macro_rules! define_accumulate_batch {
    ($name:ident, $size:literal, $(($acc:ident, $index:literal)),+ $(,)?) => {
        /// Interleaves independent hashes so load and multiply latency from one
        /// input can overlap useful work from the others.
        #[target_feature(enable = "avx2")]
        /// # Safety
        /// The caller must have detected AVX2 support. All inputs must have the
        /// same length.
        pub(in crate::xxhash::long_inputs) unsafe fn $name(
            inputs: [LongInput<'_>; $size],
            secret: &Secret,
        ) -> [[u64; 8]; $size] {
            let data = inputs.map(LongInput::as_bytes);
            let secret = secret.as_bytes();
            $(let mut $acc = unsafe { initial() };)+
            let schedule = build_long_input_schedule(inputs[0]);

            for block in 0..schedule.full_blocks() {
                let offset = block * 1024;
                if block + 2 <= schedule.full_blocks() {
                    let prefetch_offset = (block + 2) * 1024;
                    unsafe {
                        $(_mm_prefetch::<_MM_HINT_T0>(
                            data[$index].as_ptr().add(prefetch_offset).cast(),
                        );)+
                    }
                }
                for stripe in 0..16 {
                    let input_offset = offset + stripe * 64;
                    let secret_ptr = unsafe { secret.as_ptr().add(stripe * 8) };
                    unsafe {
                        $(accumulate_registers(
                            &mut $acc,
                            data[$index].as_ptr().add(input_offset),
                            secret_ptr,
                        );)+
                    }
                }
                let secret_ptr = unsafe { secret.as_ptr().add(128) };
                unsafe {
                    $(scramble_registers(&mut $acc, secret_ptr);)+
                }
            }

            for stripe in 0..schedule.tail_stripes() {
                let input_offset = schedule.tail_offset() + stripe * 64;
                let secret_ptr = unsafe { secret.as_ptr().add(stripe * 8) };
                unsafe {
                    $(accumulate_registers(
                        &mut $acc,
                        data[$index].as_ptr().add(input_offset),
                        secret_ptr,
                    );)+
                }
            }
            let input_offset = schedule.last_offset();
            let secret_ptr = unsafe { secret.as_ptr().add(121) };
            unsafe {
                $(accumulate_registers(
                    &mut $acc,
                    data[$index].as_ptr().add(input_offset),
                    secret_ptr,
                );)+
                [$(finish($acc)),+]
            }
        }
    };
}

define_accumulate_batch!(accumulate_batch2, 2, (acc0, 0), (acc1, 1));
define_accumulate_batch!(accumulate_batch3, 3, (acc0, 0), (acc1, 1), (acc2, 2));
define_accumulate_batch!(
    accumulate_batch4,
    4,
    (acc0, 0),
    (acc1, 1),
    (acc2, 2),
    (acc3, 3),
);
