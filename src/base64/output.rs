use core::mem::{ManuallyDrop, MaybeUninit};

#[inline]
pub(super) fn uninitialized_output(length: usize) -> Vec<MaybeUninit<u8>> {
    let mut output = Vec::with_capacity(length);
    // `MaybeUninit<u8>` permits every bit pattern, including uninitialized memory.
    unsafe { output.set_len(length) };
    output
}

#[inline]
pub(super) unsafe fn initialized_output(output: Vec<MaybeUninit<u8>>, length: usize) -> Vec<u8> {
    debug_assert!(length <= output.len());
    let mut output = ManuallyDrop::new(output);
    // The caller guarantees that every byte in the returned prefix was written.
    unsafe { Vec::from_raw_parts(output.as_mut_ptr().cast(), length, output.capacity()) }
}
