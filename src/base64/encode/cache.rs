//! x86-64 cache-topology policy for memory-bound encoding kernels.

#[cfg(target_arch = "x86")]
use std::arch::x86::{__cpuid, __cpuid_count};
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{__cpuid, __cpuid_count};

pub(in crate::base64) fn cached_input_limit() -> Option<usize> {
    // Encoding reads three input bytes and writes four output bytes, so 3/7 of
    // the private-cache capacity is the largest complete working set.
    detect_private_cache().map(|bytes| bytes / 7 * 3)
}

#[inline]
pub(in crate::base64) fn use_streaming_stores(
    cached_input_limit: Option<usize>,
    input_len: usize,
    output: *mut u8,
) -> bool {
    let Some(limit) = cached_input_limit else {
        return false;
    };
    input_len > limit && output.align_offset(16) == 0
}

#[allow(unused_unsafe)]
fn detect_private_cache() -> Option<usize> {
    // Leaf 4 is Intel's deterministic cache topology and is also implemented
    // by modern AMD CPUs. AMD exposes the same format at 0x8000_001d.
    // SAFETY: these CPUID leaves have no memory-safety preconditions.
    let basic_max = unsafe { __cpuid(0) }.eax;
    let extended_max = unsafe { __cpuid(0x8000_0000) }.eax;
    private_cache_from_leaves(basic_max, extended_max, deterministic_private_cache)
}

fn private_cache_from_leaves(
    basic_max: u32,
    extended_max: u32,
    read_leaf: fn(u32) -> Option<usize>,
) -> Option<usize> {
    if basic_max >= 4
        && let Some(bytes) = read_leaf(4)
    {
        return Some(bytes);
    }
    if extended_max >= 0x8000_001d {
        read_leaf(0x8000_001d)
    } else {
        None
    }
}

#[allow(unused_unsafe)]
fn deterministic_private_cache(leaf: u32) -> Option<usize> {
    let mut largest = None;
    for index in 0.. {
        // SAFETY: querying a CPUID leaf and subleaf has no memory-safety preconditions.
        let registers = unsafe { __cpuid_count(leaf, index) };
        let cache_type = registers.eax & 0x1f;
        if cache_type == 0 {
            break;
        }

        let level = (registers.eax >> 5) & 0x7;
        let is_data_or_unified = matches!(cache_type, 1 | 3);
        if level <= 2 && is_data_or_unified {
            let bytes = deterministic_cache_bytes(registers.ebx, registers.ecx);
            largest = Some(largest.map_or(bytes, |current: usize| current.max(bytes)));
        }
    }
    largest
}

fn deterministic_cache_bytes(ebx: u32, ecx: u32) -> usize {
    let line_bytes = u128::from((ebx & 0xfff) + 1);
    let partitions = u128::from(((ebx >> 12) & 0x3ff) + 1);
    let ways = u128::from((ebx >> 22) + 1);
    let sets = u128::from(ecx) + 1;
    let bytes = line_bytes * partitions * ways * sets;
    bytes.min(usize::MAX as u128) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn half_mebibyte_cache(leaf: u32) -> Option<usize> {
        assert_eq!(leaf, 4);
        Some(512 << 10)
    }

    fn two_mebibyte_cache(leaf: u32) -> Option<usize> {
        assert_eq!(leaf, 0x8000_001d);
        Some(2 << 20)
    }

    fn no_cache(_: u32) -> Option<usize> {
        None
    }

    #[test]
    fn deterministic_cache_geometry_is_decoded_without_rounding() {
        // 64-byte lines, one partition, eight ways, and 1,024 sets: 512 KiB.
        let ebx = 63 | (7 << 22);
        assert_eq!(deterministic_cache_bytes(ebx, 1023), 512 << 10);
        assert_eq!(deterministic_cache_bytes(u32::MAX, u32::MAX), usize::MAX);
    }

    #[test]
    fn basic_topology_is_preferred_and_extended_is_a_fallback() {
        assert_eq!(
            private_cache_from_leaves(4, 0x8000_001d, half_mebibyte_cache),
            Some(512 << 10)
        );
        assert_eq!(
            private_cache_from_leaves(3, 0x8000_001d, two_mebibyte_cache),
            Some(2 << 20)
        );
        assert_eq!(private_cache_from_leaves(4, 0, no_cache), None);
    }

    #[test]
    fn streaming_stores_require_cache_pressure_and_alignment() {
        let mut output = [0_u8; 32];
        let offset = output.as_mut_ptr().align_offset(16);
        let aligned = unsafe { output.as_mut_ptr().add(offset) };
        let misaligned = unsafe { aligned.add(1) };

        assert!(!use_streaming_stores(None, 8 << 20, aligned));
        assert!(!use_streaming_stores(Some(4 << 20), 1024, aligned));
        assert!(!use_streaming_stores(Some(4 << 20), 8 << 20, misaligned));
        assert!(use_streaming_stores(Some(4 << 20), 8 << 20, aligned));
    }
}
