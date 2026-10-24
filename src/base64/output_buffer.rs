//! Manage uninitialized Base64 output allocations.

use core::mem::{ManuallyDrop, MaybeUninit};

#[inline]
pub(super) fn allocate_uninitialized_output(length: usize) -> Vec<MaybeUninit<u8>> {
    Box::<[u8]>::new_uninit_slice(length).into_vec()
}

/// # Safety
/// The returned prefix must be fully initialized and length must not exceed the original allocation
/// length.
#[inline]
pub(super) unsafe fn assume_output_initialized(
    output: Vec<MaybeUninit<u8>>,
    length: usize,
) -> Vec<u8> {
    debug_assert!(length <= output.len());

    let mut output = ManuallyDrop::new(output);

    // The caller guarantees that an operation wrote every byte in the returned prefix.
    // SAFETY: The caller initialized the returned prefix; ManuallyDrop preserves the original
    // allocation, capacity, and byte alignment.
    unsafe { Vec::from_raw_parts(output.as_mut_ptr().cast(), length, output.capacity()) }
}
