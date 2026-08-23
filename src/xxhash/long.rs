#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
use crate::backend;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::Capabilities;
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::SimdBackend;

#[cfg(target_arch = "aarch64")]
mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx512;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ssse3;

use super::primitives::*;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const X86_BACKEND_PREFERENCE: [SimdBackend; 4] = [
    SimdBackend::Avx512,
    SimdBackend::Avx2,
    SimdBackend::Sse41,
    SimdBackend::Ssse3,
];

pub(super) fn init_secret_scalar(seed: u64) -> [u8; 192] {
    let mut secret = SECRET;
    for offset in (0..192).step_by(16) {
        let lo = u64le(&SECRET, offset).wrapping_add(seed);
        let hi = u64le(&SECRET, offset + 8).wrapping_sub(seed);
        secret[offset..offset + 8].copy_from_slice(&lo.to_le_bytes());
        secret[offset + 8..offset + 16].copy_from_slice(&hi.to_le_bytes());
    }
    secret
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn init_secret(seed: u64) -> [u8; 192] {
    init_secret_with_capabilities(seed, backend::capabilities())
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn init_secret_with_capabilities(seed: u64, capabilities: Capabilities) -> [u8; 192] {
    if capabilities.supports(SimdBackend::Avx2) {
        unsafe { avx2::init_secret(seed) }
    } else {
        init_secret_scalar(seed)
    }
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
#[inline]
pub(super) fn init_secret(seed: u64) -> [u8; 192] {
    init_secret_scalar(seed)
}

#[inline(always)]
pub(super) fn accumulate_scalar(acc: &mut [u64; 8], data: &[u8], secret: &[u8], offset: usize) {
    for lane in 0..8 {
        let value = u64le(data, offset + lane * 8);
        let keyed = value ^ u64le(secret, lane * 8);
        acc[lane ^ 1] = acc[lane ^ 1].wrapping_add(value);
        acc[lane] = acc[lane].wrapping_add((keyed as u32 as u64).wrapping_mul(keyed >> 32));
    }
}

#[inline(always)]
pub(super) fn scramble_scalar(acc: &mut [u64; 8], secret: &[u8]) {
    for (lane, value) in acc.iter_mut().enumerate() {
        *value ^= *value >> 47;
        *value ^= u64le(secret, lane * 8);
        *value = value.wrapping_mul(P32_1);
    }
}

pub(super) fn merge(acc: &[u64; 8], secret: &[u8], start: u64) -> u64 {
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
pub(super) fn initial_accumulator() -> [u64; 8] {
    [P32_3, P64_1, P64_2, P64_3, P64_4, P32_2, P64_5, P32_1]
}

#[derive(Clone, Copy)]
pub(super) struct LongSchedule {
    pub(super) full_blocks: usize,
    pub(super) tail_offset: usize,
    pub(super) tail_stripes: usize,
    pub(super) last_offset: usize,
}

#[inline]
pub(super) fn long_schedule(length: usize) -> LongSchedule {
    let full_blocks = (length - 1) / 1024;
    let tail_offset = full_blocks * 1024;
    LongSchedule {
        full_blocks,
        tail_offset,
        tail_stripes: (length - tail_offset - 1) / 64,
        last_offset: length - 64,
    }
}

pub(super) fn long_accumulate_scalar(data: &[u8], secret: &[u8]) -> [u64; 8] {
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

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn long_accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    let selected = backend::capabilities().best(&X86_BACKEND_PREFERENCE);
    // CPU detection above satisfies the selected kernel's target-feature contract.
    unsafe { accumulate_x86(data, secret, selected) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) unsafe fn accumulate_x86(data: &[u8], secret: &[u8], backend: SimdBackend) -> [u64; 8] {
    let Some(kernel) = x86_kernel(backend) else {
        return long_accumulate_scalar(data, secret);
    };
    unsafe { kernel(data, secret) }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type X86Kernel = unsafe fn(&[u8], &[u8]) -> [u64; 8];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn x86_kernel(backend: SimdBackend) -> Option<X86Kernel> {
    match backend {
        SimdBackend::Ssse3 | SimdBackend::Sse41 => Some(ssse3::accumulate),
        SimdBackend::Avx2 => Some(avx2::accumulate),
        SimdBackend::Avx512 => Some(avx512::accumulate),
        SimdBackend::Scalar | SimdBackend::Neon | SimdBackend::Avx512Vbmi => None,
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
pub(super) fn long_accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    if backend::capabilities().supports(SimdBackend::Neon) {
        unsafe { aarch64::accumulate(data, secret) }
    } else {
        long_accumulate_scalar(data, secret)
    }
}

#[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
#[inline]
pub(super) fn long_accumulate(data: &[u8], secret: &[u8]) -> [u64; 8] {
    long_accumulate_scalar(data, secret)
}

pub(super) fn xxh3_64_long(data: &[u8], seed: u64) -> u64 {
    let secret = (seed != 0).then(|| init_secret(seed));
    let secret = secret.as_ref().unwrap_or(&SECRET);
    let acc = long_accumulate(data, secret);
    merge(&acc, &secret[11..], (data.len() as u64).wrapping_mul(P64_1))
}

pub(super) fn xxh3_128_long(data: &[u8], seed: u64) -> [u64; 2] {
    let secret = (seed != 0).then(|| init_secret(seed));
    let secret = secret.as_ref().unwrap_or(&SECRET);
    finalize_long_128(data.len(), secret, long_accumulate(data, secret))
}

#[inline]
pub(super) fn finalize_long_128(length: usize, secret: &[u8], acc: [u64; 8]) -> [u64; 2] {
    [
        merge(&acc, &secret[11..], (length as u64).wrapping_mul(P64_1)),
        merge(&acc, &secret[117..], !(length as u64).wrapping_mul(P64_2)),
    ]
}
