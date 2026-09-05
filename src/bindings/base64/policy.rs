//! Decode policy preparation and route selection.

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes};

use super::configured::ConfiguredDecoder;
use super::lenient::lenient_decode_table;
use crate::bindings::buffer::contiguous_bytes_like;
use crate::bindings::compatibility::{PythonSemantics, python_semantics};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Padding {
    Padded,
    Unpadded,
}

impl Padding {
    pub(super) fn new(padded: bool) -> Self {
        if padded { Self::Padded } else { Self::Unpadded }
    }

    pub(super) fn is_padded(self) -> bool {
        self == Self::Padded
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Validation {
    Lenient,
    Strict,
}

impl Validation {
    pub(super) fn is_strict(self) -> bool {
        self == Self::Strict
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ErrorWrites {
    /// Direct SIMD probes may write complete validated blocks; the remaining
    /// suffix stays untouched so a lenient retry can produce a shorter result.
    /// Custom-alphabet probes validate the entire input before writing.
    ValidatedPrefix,
    /// A strict attempt may also write within the failing block.
    MayWrite,
}

impl ErrorWrites {
    pub(super) fn validated_prefix_only(self) -> bool {
        self == Self::ValidatedPrefix
    }
}

/// Probes defer capacity failures to the next decoder; strict attempts report them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeAttempt {
    Probe,
    Strict,
}

impl DecodeAttempt {
    pub(super) fn error_writes(self) -> ErrorWrites {
        match self {
            Self::Probe => ErrorWrites::ValidatedPrefix,
            Self::Strict => ErrorWrites::MayWrite,
        }
    }

    pub(super) fn accept<T>(
        self,
        result: Result<T, crate::base64::Base64Error>,
    ) -> Result<Option<T>, crate::base64::Base64Error> {
        use crate::base64::Base64Error;
        match result {
            Ok(value) => Ok(Some(value)),
            Err(error @ Base64Error::OutputTooSmall { .. }) if self == Self::Strict => Err(error),
            Err(Base64Error::InvalidInput | Base64Error::OutputTooSmall { .. }) => Ok(None),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct DecodePolicy<'a, 'py> {
    pub(super) altchars: Option<[u8; 2]>,
    validate: Option<bool>,
    pub(super) padding: Padding,
    ignorechars: Option<&'a Bound<'py, PyAny>>,
    pub(super) canonical: bool,
}

impl<'a, 'py> DecodePolicy<'a, 'py> {
    pub(super) fn new(
        altchars: Option<[u8; 2]>,
        validate: Option<bool>,
        padded: bool,
        ignorechars: Option<&'a Bound<'py, PyAny>>,
        canonical: bool,
    ) -> Self {
        Self {
            altchars,
            validate,
            padding: Padding::new(padded),
            ignorechars,
            canonical,
        }
    }

    pub(super) fn standard() -> Self {
        Self::new(None, Some(false), true, None, false)
    }

    pub(super) fn urlsafe(padded: bool) -> Self {
        Self::new(Some(*b"-_"), Some(false), padded, None, false)
    }

    fn validation(self) -> Validation {
        if self.validate.unwrap_or(self.ignorechars.is_some()) {
            Validation::Strict
        } else {
            Validation::Lenient
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfiguredShortcut {
    None,
    StandardStrict,
    CanonicalUnpadded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeRoute {
    Configured(ConfiguredShortcut),
    Strict { urlsafe_315: bool },
    LenientDirect { urlsafe_315: bool },
    LenientCustom,
}

#[derive(Clone, Copy, Default)]
pub(super) struct IgnoredBytes([u64; 4]);

impl IgnoredBytes {
    fn insert(&mut self, byte: u8) {
        let byte = usize::from(byte);
        self.0[byte / 64] |= 1_u64 << (byte % 64);
    }

    pub(super) fn contains(self, byte: u8) -> bool {
        let byte = usize::from(byte);
        self.0[byte / 64] & (1_u64 << (byte % 64)) != 0
    }
}

pub(super) struct PreparedPolicy {
    pub(super) altchars: Option<[u8; 2]>,
    pub(super) validation: Validation,
    pub(super) padding: Padding,
    pub(super) ignorechars_specified: bool,
    pub(super) ignored: Option<IgnoredBytes>,
    pub(super) canonical: bool,
}

impl PreparedPolicy {
    fn prepare(policy: DecodePolicy<'_, '_>) -> PyResult<(Self, bool)> {
        let ignorechars_specified = policy.ignorechars.is_some();
        let empty_exact_ignorechars = policy.ignorechars.is_some_and(|value| {
            value
                .cast::<PyBytes>()
                .is_ok_and(|bytes| bytes.as_bytes().is_empty())
        });
        let ignored = if let Some(ignorechars) = policy.ignorechars {
            let mut ignored = IgnoredBytes::default();
            let ignorechars = contiguous_bytes_like(ignorechars, "ignorechars")?;
            unsafe {
                ignorechars.with_bytes(|bytes| {
                    for &byte in bytes {
                        ignored.insert(byte);
                    }
                })
            };
            Some(ignored)
        } else {
            None
        };
        Ok((
            Self {
                altchars: policy.altchars,
                validation: policy.validation(),
                padding: policy.padding,
                ignorechars_specified,
                ignored,
                canonical: policy.canonical,
            },
            empty_exact_ignorechars,
        ))
    }

    pub(super) fn strict_mode(&self) -> bool {
        self.validation.is_strict()
    }

    pub(super) fn strict_custom(&self) -> Self {
        Self {
            altchars: self.altchars,
            validation: Validation::Strict,
            padding: self.padding,
            ignorechars_specified: false,
            ignored: None,
            canonical: false,
        }
    }
}

pub(super) struct PreparedDecoder {
    pub(super) semantics: PythonSemantics,
    pub(super) policy: PreparedPolicy,
    pub(super) route: DecodeRoute,
    pub(super) attempt: DecodeAttempt,
    configured: std::sync::OnceLock<Box<ConfiguredDecoder>>,
    strict_custom: std::sync::OnceLock<Box<ConfiguredDecoder>>,
    lenient_table: std::sync::OnceLock<Box<[u8; 256]>>,
}

impl PreparedDecoder {
    pub(super) fn new(py: Python<'_>, policy: DecodePolicy<'_, '_>) -> PyResult<Self> {
        let semantics = python_semantics(py);
        let (policy, empty_exact_ignorechars) = PreparedPolicy::prepare(policy)?;
        let route = select_route(&policy, empty_exact_ignorechars, semantics);
        Ok(Self {
            semantics,
            attempt: if policy.strict_mode() {
                DecodeAttempt::Strict
            } else {
                DecodeAttempt::Probe
            },
            policy,
            route,
            configured: std::sync::OnceLock::new(),
            strict_custom: std::sync::OnceLock::new(),
            lenient_table: std::sync::OnceLock::new(),
        })
    }

    pub(super) fn configured(&self) -> &ConfiguredDecoder {
        self.configured
            .get_or_init(|| Box::new(ConfiguredDecoder::new(&self.policy)))
    }

    pub(super) fn strict_custom(&self) -> &ConfiguredDecoder {
        self.strict_custom
            .get_or_init(|| Box::new(ConfiguredDecoder::new(&self.policy.strict_custom())))
    }

    pub(super) fn lenient_table(&self) -> &[u8; 256] {
        self.lenient_table
            .get_or_init(|| Box::new(lenient_decode_table(self.policy.altchars)))
    }
}

fn select_route(
    policy: &PreparedPolicy,
    empty_exact_ignorechars: bool,
    semantics: PythonSemantics,
) -> DecodeRoute {
    let standard_strict = policy.altchars.is_none()
        && policy.padding.is_padded()
        && (!policy.ignorechars_specified || empty_exact_ignorechars)
        && (policy.canonical || empty_exact_ignorechars);
    if policy.ignorechars_specified || policy.canonical {
        let shortcut = if standard_strict {
            ConfiguredShortcut::StandardStrict
        } else if !policy.ignorechars_specified && policy.canonical && !policy.padding.is_padded() {
            ConfiguredShortcut::CanonicalUnpadded
        } else {
            ConfiguredShortcut::None
        };
        return DecodeRoute::Configured(shortcut);
    }

    let urlsafe_315 = semantics.urlsafe_exclusive_alphabet && policy.altchars == Some(*b"-_");
    if policy.strict_mode() {
        return DecodeRoute::Strict { urlsafe_315 };
    }
    if matches!(policy.altchars, None | Some([b'-', b'_'])) {
        DecodeRoute::LenientDirect { urlsafe_315 }
    } else {
        DecodeRoute::LenientCustom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared_policy(
        altchars: Option<[u8; 2]>,
        validation: Validation,
        padding: Padding,
        ignorechars_specified: bool,
        canonical: bool,
    ) -> PreparedPolicy {
        PreparedPolicy {
            altchars,
            validation,
            padding,
            ignorechars_specified,
            ignored: None,
            canonical,
        }
    }

    #[test]
    fn route_selection_covers_decode_policies() {
        let old = PythonSemantics::from_version((3, 14, 4));
        let new = PythonSemantics::from_version((3, 15, 0));
        assert_eq!(
            select_route(
                &prepared_policy(None, Validation::Strict, Padding::Padded, false, false),
                false,
                old,
            ),
            DecodeRoute::Strict { urlsafe_315: false }
        );
        assert_eq!(
            select_route(
                &prepared_policy(
                    Some(*b"-_"),
                    Validation::Strict,
                    Padding::Unpadded,
                    false,
                    false,
                ),
                false,
                new,
            ),
            DecodeRoute::Strict { urlsafe_315: true }
        );
        assert_eq!(
            select_route(
                &prepared_policy(None, Validation::Lenient, Padding::Padded, false, false),
                false,
                old,
            ),
            DecodeRoute::LenientDirect { urlsafe_315: false }
        );
        assert_eq!(
            select_route(
                &prepared_policy(
                    Some(*b"@#"),
                    Validation::Lenient,
                    Padding::Unpadded,
                    false,
                    false,
                ),
                false,
                old,
            ),
            DecodeRoute::LenientCustom
        );
        assert_eq!(
            select_route(
                &prepared_policy(None, Validation::Lenient, Padding::Padded, false, true),
                false,
                old,
            ),
            DecodeRoute::Configured(ConfiguredShortcut::StandardStrict)
        );
        assert_eq!(
            select_route(
                &prepared_policy(None, Validation::Lenient, Padding::Unpadded, false, true),
                false,
                old,
            ),
            DecodeRoute::Configured(ConfiguredShortcut::CanonicalUnpadded)
        );
        assert_eq!(
            select_route(
                &prepared_policy(
                    Some(*b"@#"),
                    Validation::Strict,
                    Padding::Padded,
                    true,
                    false,
                ),
                false,
                old,
            ),
            DecodeRoute::Configured(ConfiguredShortcut::None)
        );
    }

    #[test]
    fn attempts_bound_probe_writes_and_report_strict_capacity_errors() {
        use crate::base64::Base64Error;
        let small = Base64Error::OutputTooSmall {
            required: 3,
            provided: 2,
        };
        assert_eq!(
            DecodeAttempt::Probe.error_writes(),
            ErrorWrites::ValidatedPrefix
        );
        assert_eq!(DecodeAttempt::Strict.error_writes(), ErrorWrites::MayWrite);
        assert_eq!(DecodeAttempt::Probe.accept::<usize>(Err(small)), Ok(None));
        assert_eq!(
            DecodeAttempt::Strict.accept::<usize>(Err(small)),
            Err(small)
        );
        for attempt in [DecodeAttempt::Probe, DecodeAttempt::Strict] {
            assert_eq!(
                attempt.accept::<usize>(Err(Base64Error::InvalidInput)),
                Ok(None)
            );
            assert_eq!(attempt.accept(Ok(3)), Ok(Some(3)));
        }
    }
}
