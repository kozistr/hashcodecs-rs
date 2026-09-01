//! Detect raw CPU features used by algorithm-owned backend selectors.

#[cfg(not(any(kani, miri)))]
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CpuFeature {
    Neon,
    Ssse3,
    Sse41,
    Avx2,
    Avx512F,
    Avx512Bw,
    Avx512Vbmi,
    Bmi2,
}

impl CpuFeature {
    const fn bit(self) -> u16 {
        match self {
            Self::Neon => 1 << 0,
            Self::Ssse3 => 1 << 1,
            Self::Sse41 => 1 << 2,
            Self::Avx2 => 1 << 3,
            Self::Avx512F => 1 << 4,
            Self::Avx512Bw => 1 << 5,
            Self::Avx512Vbmi => 1 << 6,
            Self::Bmi2 => 1 << 7,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Capabilities {
    features: u16,
}

impl Capabilities {
    #[cfg(any(
        kani,
        miri,
        not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))
    ))]
    const SCALAR: Self = Self { features: 0 };

    #[inline]
    pub(crate) const fn supports(self, feature: CpuFeature) -> bool {
        self.features & feature.bit() != 0
    }

    #[inline]
    pub(crate) fn supports_all(self, features: &[CpuFeature]) -> bool {
        features.iter().all(|&feature| self.supports(feature))
    }

    #[cfg(test)]
    pub(crate) fn from_features(features: &[CpuFeature]) -> Self {
        let mut bits = 0;
        for feature in features {
            bits |= feature.bit();
        }
        Self { features: bits }
    }

    #[cfg(any(
        test,
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "x86_64"
    ))]
    fn from_feature_flags(flags: &[(CpuFeature, bool)]) -> Self {
        let mut features = 0;
        for &(feature, available) in flags {
            if available {
                features |= feature.bit();
            }
        }
        Self { features }
    }
}

#[cfg(not(any(kani, miri)))]
static CAPABILITIES: OnceLock<Capabilities> = OnceLock::new();

#[cfg(not(any(kani, miri)))]
#[inline]
pub(crate) fn capabilities() -> Capabilities {
    *CAPABILITIES.get_or_init(detect_cpu_capabilities)
}

#[cfg(any(kani, miri))]
#[inline]
pub(crate) const fn capabilities() -> Capabilities {
    Capabilities::SCALAR
}

#[cfg(not(any(kani, miri)))]
fn detect_cpu_capabilities() -> Capabilities {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        x86_capabilities_from_feature_flags(
            std::is_x86_feature_detected!("avx512vbmi"),
            std::is_x86_feature_detected!("avx512bw"),
            std::is_x86_feature_detected!("avx512f"),
            std::is_x86_feature_detected!("avx2"),
            std::is_x86_feature_detected!("sse4.1"),
            std::is_x86_feature_detected!("ssse3"),
            std::is_x86_feature_detected!("bmi2"),
        )
    }
    #[cfg(target_arch = "aarch64")]
    {
        aarch64_capabilities_from_feature_flag(std::arch::is_aarch64_feature_detected!("neon"))
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
    {
        Capabilities::SCALAR
    }
}

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
fn x86_capabilities_from_feature_flags(
    avx512_vbmi: bool,
    avx512_bw: bool,
    avx512_f: bool,
    avx2: bool,
    sse41: bool,
    ssse3: bool,
    bmi2: bool,
) -> Capabilities {
    Capabilities::from_feature_flags(&[
        (CpuFeature::Avx512Vbmi, avx512_vbmi),
        (CpuFeature::Avx512Bw, avx512_bw),
        (CpuFeature::Avx512F, avx512_f),
        (CpuFeature::Avx2, avx2),
        (CpuFeature::Sse41, sse41),
        (CpuFeature::Ssse3, ssse3),
        (CpuFeature::Bmi2, bmi2),
    ])
}

#[cfg(any(test, target_arch = "aarch64"))]
fn aarch64_capabilities_from_feature_flag(neon: bool) -> Capabilities {
    Capabilities::from_feature_flags(&[(CpuFeature::Neon, neon)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_detection_keeps_raw_features_independent() {
        let capabilities =
            x86_capabilities_from_feature_flags(true, false, true, true, true, false, true);
        assert!(capabilities.supports(CpuFeature::Avx512Vbmi));
        assert!(!capabilities.supports(CpuFeature::Avx512Bw));
        assert!(capabilities.supports(CpuFeature::Avx512F));
        assert!(capabilities.supports(CpuFeature::Avx2));
        assert!(capabilities.supports(CpuFeature::Sse41));
        assert!(!capabilities.supports(CpuFeature::Ssse3));
        assert!(capabilities.supports(CpuFeature::Bmi2));
    }

    #[test]
    fn capabilities_check_algorithm_prerequisite_sets() {
        let capabilities =
            Capabilities::from_features(&[CpuFeature::Avx2, CpuFeature::Sse41, CpuFeature::Bmi2]);
        assert!(capabilities.supports_all(&[CpuFeature::Avx2, CpuFeature::Bmi2]));
        assert!(!capabilities.supports_all(&[CpuFeature::Avx2, CpuFeature::Ssse3]));

        let neon = aarch64_capabilities_from_feature_flag(true);
        assert!(neon.supports(CpuFeature::Neon));
        assert!(!neon.supports(CpuFeature::Avx2));
        assert!(!aarch64_capabilities_from_feature_flag(false).supports(CpuFeature::Neon));
    }
}
