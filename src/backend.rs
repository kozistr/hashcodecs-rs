//! Process-wide CPU capability detection shared by algorithm dispatchers.

#[cfg(not(any(kani, miri)))]
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SimdBackend {
    Scalar,
    Neon,
    Ssse3,
    Sse41,
    Avx2,
    Avx512,
    Avx512Vbmi,
}

impl SimdBackend {
    const fn bit(self) -> u8 {
        match self {
            Self::Scalar => 0,
            Self::Neon => 1 << 0,
            Self::Ssse3 => 1 << 1,
            Self::Sse41 => 1 << 2,
            Self::Avx2 => 1 << 3,
            Self::Avx512 => 1 << 4,
            Self::Avx512Vbmi => 1 << 5,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Capabilities {
    simd: u8,
    bmi2: bool,
}

impl Capabilities {
    #[cfg(any(
        kani,
        miri,
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))
    ))]
    const SCALAR: Self = Self {
        simd: 0,
        bmi2: false,
    };

    #[inline]
    pub(crate) const fn supports(self, backend: SimdBackend) -> bool {
        let bit = backend.bit();
        bit == 0 || self.simd & bit != 0
    }

    #[inline]
    pub(crate) fn best(self, preferred: &[SimdBackend]) -> SimdBackend {
        preferred
            .iter()
            .copied()
            .find(|backend| self.supports(*backend))
            .unwrap_or(SimdBackend::Scalar)
    }

    #[inline]
    pub(crate) const fn has_bmi2(self) -> bool {
        self.bmi2
    }

    #[cfg(test)]
    pub(crate) fn for_backends(backends: &[SimdBackend]) -> Self {
        let mut simd = 0;
        for backend in backends {
            simd |= backend.bit();
        }
        Self { simd, bmi2: false }
    }
}

#[cfg(not(any(kani, miri)))]
static CAPABILITIES: OnceLock<Capabilities> = OnceLock::new();

#[cfg(not(any(kani, miri)))]
#[inline]
pub(crate) fn capabilities() -> Capabilities {
    *CAPABILITIES.get_or_init(detect)
}

#[cfg(any(kani, miri))]
#[inline]
pub(crate) const fn capabilities() -> Capabilities {
    Capabilities::SCALAR
}

#[cfg(not(any(kani, miri)))]
fn detect() -> Capabilities {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        x86_capabilities(
            std::is_x86_feature_detected!("avx512vbmi"),
            std::is_x86_feature_detected!("avx512f"),
            std::is_x86_feature_detected!("avx2"),
            std::is_x86_feature_detected!("sse4.1"),
            std::is_x86_feature_detected!("ssse3"),
            std::is_x86_feature_detected!("bmi2"),
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        aarch64_capabilities(std::arch::is_aarch64_feature_detected!("neon"))
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    {
        Capabilities::SCALAR
    }
}

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
fn x86_capabilities(
    avx512_vbmi: bool,
    avx512: bool,
    avx2: bool,
    sse41: bool,
    ssse3: bool,
    bmi2: bool,
) -> Capabilities {
    let mut simd = 0;
    if ssse3 {
        simd |= SimdBackend::Ssse3.bit();
    }
    // The SSE4.1 kernels also contain SSSE3 instructions.
    if sse41 && ssse3 {
        simd |= SimdBackend::Sse41.bit();
    }
    if avx2 {
        simd |= SimdBackend::Avx2.bit();
    }
    if avx512 {
        simd |= SimdBackend::Avx512.bit();
    }
    if avx512 && avx512_vbmi {
        simd |= SimdBackend::Avx512Vbmi.bit();
    }
    Capabilities { simd, bmi2 }
}

#[cfg(any(test, target_arch = "aarch64"))]
fn aarch64_capabilities(neon: bool) -> Capabilities {
    Capabilities {
        simd: if neon { SimdBackend::Neon.bit() } else { 0 },
        bmi2: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_detection_tracks_independent_feature_sets() {
        let scalar = x86_capabilities(false, false, false, false, false, false);
        assert_eq!(scalar.best(&[SimdBackend::Avx2]), SimdBackend::Scalar);

        let ssse3 = x86_capabilities(false, false, false, false, true, false);
        assert!(ssse3.supports(SimdBackend::Ssse3));
        assert!(!ssse3.supports(SimdBackend::Sse41));

        let incomplete_sse41 = x86_capabilities(false, false, false, true, false, false);
        assert!(!incomplete_sse41.supports(SimdBackend::Sse41));

        let sse41 = x86_capabilities(false, false, false, true, true, false);
        assert!(sse41.supports(SimdBackend::Sse41));

        let avx2 = x86_capabilities(false, false, true, false, false, true);
        assert!(avx2.supports(SimdBackend::Avx2));
        assert!(!avx2.supports(SimdBackend::Sse41));
        assert!(avx2.has_bmi2());

        let avx512 = x86_capabilities(false, true, false, false, false, false);
        assert!(avx512.supports(SimdBackend::Avx512));
        assert!(!avx512.supports(SimdBackend::Avx2));
        assert!(!avx512.supports(SimdBackend::Avx512Vbmi));

        let incomplete_vbmi = x86_capabilities(true, false, false, false, false, false);
        assert!(!incomplete_vbmi.supports(SimdBackend::Avx512Vbmi));

        let vbmi = x86_capabilities(true, true, true, true, true, true);
        assert_eq!(
            vbmi.best(&[
                SimdBackend::Avx512Vbmi,
                SimdBackend::Avx512,
                SimdBackend::Avx2,
            ]),
            SimdBackend::Avx512Vbmi
        );
    }

    #[test]
    fn selection_respects_architecture_boundaries_and_preference_order() {
        let neon = aarch64_capabilities(true);
        assert!(neon.supports(SimdBackend::Neon));
        assert!(neon.supports(SimdBackend::Scalar));
        assert!(!neon.supports(SimdBackend::Ssse3));
        assert_eq!(
            neon.best(&[SimdBackend::Avx2, SimdBackend::Neon]),
            SimdBackend::Neon
        );
        assert_eq!(
            aarch64_capabilities(false).best(&[SimdBackend::Neon]),
            SimdBackend::Scalar
        );

        let mixed = Capabilities::for_backends(&[
            SimdBackend::Ssse3,
            SimdBackend::Avx2,
            SimdBackend::Avx512,
        ]);
        assert_eq!(
            mixed.best(&[SimdBackend::Avx2, SimdBackend::Avx512]),
            SimdBackend::Avx2
        );
        assert!(capabilities().supports(SimdBackend::Scalar));
    }
}
