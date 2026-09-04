use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes};

use super::configured::ConfiguredDecoder;
use super::lenient::lenient_decode_table;
use crate::bindings::base64::{PythonSemantics, python_semantics};
use crate::bindings::buffer::contiguous_bytes_like;

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
    PreserveOutput,
    MayWrite,
}

impl ErrorWrites {
    pub(super) fn transactional(self) -> bool {
        self == Self::PreserveOutput
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StoreBounds {
    DeferToFallback,
    ReportImmediately,
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

pub(super) struct PreparedPolicy {
    pub(super) altchars: Option<[u8; 2]>,
    pub(super) validation: Validation,
    pub(super) padding: Padding,
    pub(super) ignorechars_specified: bool,
    pub(super) ignored: Option<Box<[bool; 256]>>,
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
            let mut ignored = [false; 256];
            let ignorechars = contiguous_bytes_like(ignorechars, "ignorechars")?;
            unsafe {
                ignorechars.with_bytes(|bytes| {
                    for &byte in bytes {
                        ignored[usize::from(byte)] = true;
                    }
                })
            };
            Some(Box::new(ignored))
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

    pub(super) fn error_writes(&self) -> ErrorWrites {
        if self.strict_mode() {
            ErrorWrites::MayWrite
        } else {
            ErrorWrites::PreserveOutput
        }
    }

    pub(super) fn store_bounds(&self) -> StoreBounds {
        if self.strict_mode() {
            StoreBounds::ReportImmediately
        } else {
            StoreBounds::DeferToFallback
        }
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
    fn policy_enums_expose_write_and_bound_semantics() {
        let lenient = prepared_policy(None, Validation::Lenient, Padding::Padded, false, false);
        let strict = prepared_policy(None, Validation::Strict, Padding::Padded, false, false);
        assert_eq!(lenient.error_writes(), ErrorWrites::PreserveOutput);
        assert_eq!(lenient.store_bounds(), StoreBounds::DeferToFallback);
        assert_eq!(strict.error_writes(), ErrorWrites::MayWrite);
        assert_eq!(strict.store_bounds(), StoreBounds::ReportImmediately);
    }
}
