//! Process XXH3 inputs that contain more than 240 bytes.

#[cfg(not(any(kani, miri)))]
use std::sync::OnceLock;

#[cfg(any(
    all(target_arch = "aarch64", target_endian = "little"),
    target_arch = "x86",
    target_arch = "x86_64"
))]
use crate::backend;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use crate::backend::{Capabilities, CpuFeature};

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod aarch64;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx2_batch;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod avx512;
mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod ssse3;

use super::primitives::*;

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
pub(super) fn accumulate_long_input_scalar(input: LongInput<'_>, secret: &Secret) -> [u64; 8] {
    scalar::accumulate(input, secret)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const X86_BACKEND_PREFERENCE: [X86Backend; 4] = [
    X86Backend::Avx512,
    X86Backend::Avx2,
    X86Backend::Ssse3,
    X86Backend::Scalar,
];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum X86Backend {
    Scalar,
    Ssse3,
    Avx2,
    Avx512,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn select_x86_backend(capabilities: Capabilities) -> X86Backend {
    X86_BACKEND_PREFERENCE
        .into_iter()
        .find(|&backend| x86_backend_supported(capabilities, backend))
        .expect("scalar XXH3 is always available")
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
fn x86_backend_supported(capabilities: Capabilities, backend: X86Backend) -> bool {
    match backend {
        X86Backend::Scalar => true,
        X86Backend::Ssse3 => capabilities.supports(CpuFeature::Ssse3),
        X86Backend::Avx2 => capabilities.supports(CpuFeature::Avx2),
        X86Backend::Avx512 => capabilities.supports(CpuFeature::Avx512F),
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LongInput<'a>(&'a [u8]);

impl<'a> LongInput<'a> {
    #[inline]
    pub(super) fn new(input: &'a [u8]) -> Option<Self> {
        (input.len() > 240).then_some(Self(input))
    }

    #[inline(always)]
    pub(super) fn as_bytes(self) -> &'a [u8] {
        self.0
    }

    #[inline(always)]
    pub(super) fn len(self) -> usize {
        self.0.len()
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LongBatch<'a, const N: usize>([LongInput<'a>; N]);

impl<'a, const N: usize> LongBatch<'a, N> {
    #[inline(always)]
    pub(super) fn into_inputs(self) -> [LongInput<'a>; N] {
        self.0
    }

    #[inline(always)]
    pub(super) fn input(self, index: usize) -> LongInput<'a> {
        self.0[index]
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LongRun<'batch, 'input> {
    inputs: &'batch [&'input [u8]],
}

impl<'batch, 'input> LongRun<'batch, 'input> {
    #[inline(always)]
    pub(super) fn new(inputs: &'batch [&'input [u8]]) -> Option<Self> {
        let first = LongInput::new(inputs.first()?)?;
        let run_length = inputs
            .iter()
            .take_while(|input| input.len() == first.len())
            .count();
        Some(Self {
            inputs: &inputs[..run_length],
        })
    }

    #[inline(always)]
    pub(super) fn len(self) -> usize {
        self.inputs.len()
    }

    #[inline(always)]
    pub(super) fn input(self, index: usize) -> LongInput<'input> {
        LongInput(self.inputs[index])
    }

    #[inline(always)]
    pub(super) fn batch2(self, index: usize) -> LongBatch<'input, 2> {
        LongBatch([self.input(index), self.input(index + 1)])
    }

    #[inline(always)]
    pub(super) fn batch3(self, index: usize) -> LongBatch<'input, 3> {
        LongBatch([
            self.input(index),
            self.input(index + 1),
            self.input(index + 2),
        ])
    }

    #[inline(always)]
    pub(super) fn batch4(self, index: usize) -> LongBatch<'input, 4> {
        LongBatch([
            self.input(index),
            self.input(index + 1),
            self.input(index + 2),
            self.input(index + 3),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Secret([u8; 192]);

impl Secret {
    #[inline(always)]
    pub(super) fn as_bytes(&self) -> &[u8; 192] {
        &self.0
    }
}

static DEFAULT_SECRET: Secret = Secret(SECRET);

#[inline]
pub(super) fn initialize_secret_scalar(seed: u64) -> Secret {
    let mut secret = SECRET;
    for offset in (0..192).step_by(16) {
        let lo = read_u64_le(&SECRET, offset).wrapping_add(seed);
        let hi = read_u64_le(&SECRET, offset + 8).wrapping_sub(seed);
        secret[offset..offset + 8].copy_from_slice(&lo.to_le_bytes());
        secret[offset + 8..offset + 16].copy_from_slice(&hi.to_le_bytes());
    }
    Secret(secret)
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg(test)]
#[inline]
pub(super) fn initialize_secret_with_capabilities(seed: u64, capabilities: Capabilities) -> Secret {
    if capabilities.supports(CpuFeature::Avx2) {
        unsafe { avx2::init_secret(seed) }
    } else {
        initialize_secret_scalar(seed)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LongSchedule {
    full_blocks: usize,
    tail_offset: usize,
    tail_stripes: usize,
    last_offset: usize,
}

impl LongSchedule {
    #[inline(always)]
    pub(super) const fn full_blocks(self) -> usize {
        self.full_blocks
    }

    #[inline(always)]
    pub(super) const fn tail_offset(self) -> usize {
        self.tail_offset
    }

    #[inline(always)]
    pub(super) const fn tail_stripes(self) -> usize {
        self.tail_stripes
    }

    #[inline(always)]
    pub(super) const fn last_offset(self) -> usize {
        self.last_offset
    }
}

#[inline]
pub(super) fn build_long_input_schedule(input: LongInput<'_>) -> LongSchedule {
    let length = input.len();
    let full_blocks = (length - 1) / 1024;
    let tail_offset = full_blocks * 1024;
    LongSchedule {
        full_blocks,
        tail_offset,
        tail_stripes: (length - tail_offset - 1) / 64,
        last_offset: length - 64,
    }
}

#[inline]
pub(super) fn initial_accumulator() -> [u64; 8] {
    [P32_3, P64_1, P64_2, P64_3, P64_4, P32_2, P64_5, P32_1]
}

#[inline(always)]
pub(super) fn merge(acc: &[u64; 8], secret: &Secret, offset: usize, start: u64) -> u64 {
    let secret = secret.as_bytes();
    let mut result = start;
    for lane in 0..4 {
        result = result.wrapping_add(mul_fold(
            acc[lane * 2] ^ read_u64_le(secret, offset + lane * 16),
            acc[lane * 2 + 1] ^ read_u64_le(secret, offset + lane * 16 + 8),
        ));
    }
    avalanche(result)
}

#[inline(always)]
pub(super) fn finalize_long_64(length: usize, secret: &Secret, acc: [u64; 8]) -> u64 {
    merge(&acc, secret, 11, (length as u64).wrapping_mul(P64_1))
}

#[inline(always)]
pub(super) fn finalize_long_128(length: usize, secret: &Secret, acc: [u64; 8]) -> [u64; 2] {
    [
        merge(&acc, secret, 11, (length as u64).wrapping_mul(P64_1)),
        merge(&acc, secret, 117, !(length as u64).wrapping_mul(P64_2)),
    ]
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
type X86Kernel = unsafe fn(LongInput<'_>, &Secret) -> [u64; 8];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn select_x86_accumulation_kernel(backend: X86Backend) -> Option<X86Kernel> {
    match backend {
        X86Backend::Ssse3 => Some(ssse3::accumulate),
        X86Backend::Avx2 => Some(avx2::accumulate),
        X86Backend::Avx512 => Some(avx512::accumulate),
        X86Backend::Scalar => None,
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[cfg(test)]
#[inline]
pub(super) unsafe fn accumulate_x86(
    input: LongInput<'_>,
    secret: &Secret,
    backend: X86Backend,
) -> [u64; 8] {
    let Some(kernel) = select_x86_accumulation_kernel(backend) else {
        return scalar::accumulate(input, secret);
    };
    unsafe { kernel(input, secret) }
}

#[derive(Clone, Copy)]
enum LongBackend {
    Scalar,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    X86(X86Kernel),
    #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
    Neon,
}

pub(super) struct LongEngine {
    backend: LongBackend,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    avx2_available: bool,
}

#[cfg(not(any(kani, miri)))]
static LONG_ENGINE: OnceLock<LongEngine> = OnceLock::new();

#[cfg(any(kani, miri))]
static LONG_ENGINE: LongEngine = LongEngine {
    backend: LongBackend::Scalar,
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    avx2_available: false,
};

impl LongEngine {
    #[inline(always)]
    pub(super) fn cached() -> &'static Self {
        #[cfg(not(any(kani, miri)))]
        {
            LONG_ENGINE.get_or_init(Self::new)
        }
        #[cfg(any(kani, miri))]
        {
            &LONG_ENGINE
        }
    }

    #[inline(always)]
    pub(super) fn new() -> Self {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            Self::new_with_capabilities(backend::capabilities())
        }

        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        {
            let backend = if backend::capabilities().supports(crate::backend::CpuFeature::Neon) {
                LongBackend::Neon
            } else {
                LongBackend::Scalar
            };
            Self { backend }
        }

        #[cfg(not(any(
            all(target_arch = "aarch64", target_endian = "little"),
            target_arch = "x86",
            target_arch = "x86_64"
        )))]
        Self {
            backend: LongBackend::Scalar,
        }
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    #[inline(always)]
    pub(super) fn new_with_capabilities(capabilities: Capabilities) -> Self {
        let selected = select_x86_backend(capabilities);
        let backend = match select_x86_accumulation_kernel(selected) {
            Some(kernel) => LongBackend::X86(kernel),
            None => LongBackend::Scalar,
        };
        Self {
            backend,
            avx2_available: capabilities.supports(CpuFeature::Avx2),
        }
    }

    #[inline]
    pub(super) fn derive_secret(&self, seed: u64) -> Option<Secret> {
        if seed == 0 {
            return None;
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        if self.avx2_available {
            return Some(unsafe { avx2::init_secret(seed) });
        }
        Some(initialize_secret_scalar(seed))
    }

    #[inline(always)]
    pub(super) fn secret<'a>(&self, derived: &'a Option<Secret>) -> &'a Secret {
        derived.as_ref().unwrap_or(&DEFAULT_SECRET)
    }

    #[inline(always)]
    pub(super) fn has_batch_kernel(&self) -> bool {
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            self.avx2_available
        }
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            false
        }
    }

    #[inline(always)]
    pub(super) fn accumulate(&self, input: LongInput<'_>, secret: &Secret) -> [u64; 8] {
        let backend = self.backend;
        match backend {
            LongBackend::Scalar => scalar::accumulate(input, secret),
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            LongBackend::X86(kernel) => unsafe { kernel(input, secret) },
            #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
            LongBackend::Neon => unsafe { aarch64::accumulate(input, secret) },
        }
    }

    #[inline(always)]
    pub(super) fn accumulate_batch2(
        &self,
        inputs: LongBatch<'_, 2>,
        secret: &Secret,
    ) -> [[u64; 8]; 2] {
        if let Some(accumulators) = self.try_accumulate_batch2(inputs, secret) {
            return accumulators;
        }
        let [first, second] = inputs.into_inputs();
        [
            self.accumulate(first, secret),
            self.accumulate(second, secret),
        ]
    }

    #[inline(always)]
    pub(super) fn accumulate_batch3(
        &self,
        inputs: LongBatch<'_, 3>,
        secret: &Secret,
    ) -> [[u64; 8]; 3] {
        if let Some(accumulators) = self.try_accumulate_batch3(inputs, secret) {
            return accumulators;
        }
        let [first, second, third] = inputs.into_inputs();
        [
            self.accumulate(first, secret),
            self.accumulate(second, secret),
            self.accumulate(third, secret),
        ]
    }

    #[inline(always)]
    pub(super) fn accumulate_batch4(
        &self,
        inputs: LongBatch<'_, 4>,
        secret: &Secret,
    ) -> [[u64; 8]; 4] {
        if let Some(accumulators) = self.try_accumulate_batch4(inputs, secret) {
            return accumulators;
        }
        let [first, second, third, fourth] = inputs.into_inputs();
        [
            self.accumulate(first, secret),
            self.accumulate(second, secret),
            self.accumulate(third, secret),
            self.accumulate(fourth, secret),
        ]
    }

    #[inline(always)]
    pub(super) fn hash<T>(
        &self,
        input: LongInput<'_>,
        secret: &Secret,
        finalize: impl FnOnce(usize, &Secret, [u64; 8]) -> T,
    ) -> T {
        let acc = self.accumulate(input, secret);
        finalize(input.len(), secret, acc)
    }

    #[inline(always)]
    pub(super) fn try_accumulate_batch2(
        &self,
        inputs: LongBatch<'_, 2>,
        secret: &Secret,
    ) -> Option<[[u64; 8]; 2]> {
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _ = (self, inputs, secret);
            None
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if !self.avx2_available {
                return None;
            }
            Some(unsafe { avx2_batch::accumulate_batch2(inputs.into_inputs(), secret) })
        }
    }

    #[inline(always)]
    pub(super) fn try_accumulate_batch3(
        &self,
        inputs: LongBatch<'_, 3>,
        secret: &Secret,
    ) -> Option<[[u64; 8]; 3]> {
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _ = (self, inputs, secret);
            None
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if !self.avx2_available {
                return None;
            }
            Some(unsafe { avx2_batch::accumulate_batch3(inputs.into_inputs(), secret) })
        }
    }

    #[inline(always)]
    pub(super) fn try_accumulate_batch4(
        &self,
        inputs: LongBatch<'_, 4>,
        secret: &Secret,
    ) -> Option<[[u64; 8]; 4]> {
        #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
        {
            let _ = (self, inputs, secret);
            None
        }
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        {
            if !self.avx2_available {
                return None;
            }
            Some(unsafe { avx2_batch::accumulate_batch4(inputs.into_inputs(), secret) })
        }
    }
}

pub(super) fn xxh3_64_over_240_bytes(input: LongInput<'_>, seed: u64) -> u64 {
    let engine = LongEngine::cached();
    let derived = engine.derive_secret(seed);
    engine.hash(input, engine.secret(&derived), finalize_long_64)
}

pub(super) fn xxh3_128_over_240_bytes(input: LongInput<'_>, seed: u64) -> [u64; 2] {
    let engine = LongEngine::cached();
    let derived = engine.derive_secret(seed);
    engine.hash(input, engine.secret(&derived), finalize_long_128)
}
