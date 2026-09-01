use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::fallback::warn_legacy_altchars;
use super::{decode_plan_allocating_inner, decode_plan_into_inner};
use crate::bindings::base64::python_at_least;
use crate::bindings::buffer::BytesLike;

#[derive(Clone, Copy)]
pub(super) struct DecodeOptions<'a, 'py> {
    pub(super) altchars: Option<[u8; 2]>,
    pub(super) validate: Option<bool>,
    pub(super) padded: bool,
    pub(super) ignorechars: Option<&'a Bound<'py, PyAny>>,
    pub(super) canonical: bool,
}

impl<'a, 'py> DecodeOptions<'a, 'py> {
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
            padded,
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

    pub(super) fn strict_mode(self) -> bool {
        self.validate.unwrap_or(self.ignorechars.is_some())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(super) enum DecodeAttempt {
    Urlsafe315,
    StandardStrict,
    Strict,
    StrictProbe,
    MimeWhitespace,
    Unpadded,
    Lenient,
    CanonicalUnpadded,
    Advanced,
    Binascii,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DecodeStrategy {
    Advanced,
    StandardStrictThenAdvanced,
    CanonicalUnpaddedThenAdvanced,
    StrictPadded,
    StrictUnpadded,
    Urlsafe315StrictPadded,
    Urlsafe315StrictUnpadded,
    LenientDirectPadded,
    LenientDirectUnpadded,
    Urlsafe315LenientDirectPadded,
    Urlsafe315LenientDirectUnpadded,
    LenientCustomPadded,
    LenientCustomUnpadded,
}

#[derive(Clone, Copy, Default)]
struct DecodeConfiguration {
    altchars: Option<[u8; 2]>,
    padded: bool,
    strict_mode: bool,
    ignorechars_specified: bool,
    empty_ignorechars: bool,
    canonical: bool,
    python_315: bool,
}

macro_rules! select_decode_strategy {
    ($configuration:expr, $select:ident) => {{
        let DecodeConfiguration {
            altchars,
            padded,
            strict_mode,
            ignorechars_specified,
            empty_ignorechars,
            canonical,
            python_315,
        } = $configuration;
        let standard_strict = altchars.is_none()
            && padded
            && (!ignorechars_specified || empty_ignorechars)
            && (canonical || empty_ignorechars);
        if ignorechars_specified || canonical {
            if standard_strict {
                $select!(DecodeStrategy::StandardStrictThenAdvanced)
            } else if !ignorechars_specified && canonical && !padded {
                $select!(DecodeStrategy::CanonicalUnpaddedThenAdvanced)
            } else {
                $select!(DecodeStrategy::Advanced)
            }
        } else {
            let urlsafe_315 = python_315 && altchars == Some(*b"-_");
            if strict_mode {
                match (urlsafe_315, padded) {
                    (false, true) => $select!(DecodeStrategy::StrictPadded),
                    (false, false) => $select!(DecodeStrategy::StrictUnpadded),
                    (true, true) => $select!(DecodeStrategy::Urlsafe315StrictPadded),
                    (true, false) => $select!(DecodeStrategy::Urlsafe315StrictUnpadded),
                }
            } else {
                let direct = matches!(altchars, None | Some([b'-', b'_']));
                match (urlsafe_315, direct, padded) {
                    (false, true, true) => $select!(DecodeStrategy::LenientDirectPadded),
                    (false, true, false) => $select!(DecodeStrategy::LenientDirectUnpadded),
                    (true, true, true) => {
                        $select!(DecodeStrategy::Urlsafe315LenientDirectPadded)
                    }
                    (true, true, false) => {
                        $select!(DecodeStrategy::Urlsafe315LenientDirectUnpadded)
                    }
                    (false, false, true) => $select!(DecodeStrategy::LenientCustomPadded),
                    (false, false, false) => $select!(DecodeStrategy::LenientCustomUnpadded),
                    (true, false, _) => {
                        unreachable!("Python 3.15 routing requires the URL-safe alphabet")
                    }
                }
            }
        }
    }};
}

macro_rules! execute_decode_strategy {
    ($strategy:expr, $execute_attempt:ident) => {{
        match $strategy {
            DecodeStrategy::Advanced => {
                $execute_attempt!(Advanced);
            }
            DecodeStrategy::StandardStrictThenAdvanced => {
                $execute_attempt!(StandardStrict);
                $execute_attempt!(Advanced);
            }
            DecodeStrategy::CanonicalUnpaddedThenAdvanced => {
                $execute_attempt!(CanonicalUnpadded);
                $execute_attempt!(Advanced);
            }
            DecodeStrategy::StrictPadded => {
                $execute_attempt!(Strict);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::StrictUnpadded => {
                $execute_attempt!(Unpadded);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::Urlsafe315StrictPadded => {
                $execute_attempt!(Urlsafe315);
                $execute_attempt!(Strict);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::Urlsafe315StrictUnpadded => {
                $execute_attempt!(Urlsafe315);
                $execute_attempt!(Unpadded);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::LenientDirectPadded => {
                $execute_attempt!(StrictProbe);
                $execute_attempt!(MimeWhitespace);
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::LenientDirectUnpadded => {
                $execute_attempt!(StrictProbe);
                $execute_attempt!(Unpadded);
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::Urlsafe315LenientDirectPadded => {
                $execute_attempt!(Urlsafe315);
                $execute_attempt!(StrictProbe);
                $execute_attempt!(MimeWhitespace);
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::Urlsafe315LenientDirectUnpadded => {
                $execute_attempt!(Urlsafe315);
                $execute_attempt!(StrictProbe);
                $execute_attempt!(Unpadded);
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::LenientCustomPadded => {
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
            DecodeStrategy::LenientCustomUnpadded => {
                $execute_attempt!(Unpadded);
                $execute_attempt!(Lenient);
                $execute_attempt!(Binascii);
            }
        }
    }};
}

macro_rules! execute_strict_decode_strategy {
    ($py:expr, $options:expr, $execute_attempt:ident) => {{
        let options = $options;
        if options.ignorechars.is_none() && !options.canonical && options.strict_mode() {
            if options.altchars == Some(*b"-_")
                && $crate::bindings::base64::python_at_least($py, (3, 15))
            {
                $execute_attempt!(Urlsafe315);
            }
            if options.padded {
                $execute_attempt!(Strict);
            } else {
                $execute_attempt!(Unpadded);
            }
            $execute_attempt!(Binascii);
        }
    }};
}

pub(super) use {execute_decode_strategy, execute_strict_decode_strategy};

impl DecodeConfiguration {
    #[inline(always)]
    fn from_options(py: Python<'_>, options: DecodeOptions<'_, '_>) -> Self {
        let empty_ignorechars = options.ignorechars.is_some_and(|value| {
            value
                .cast::<PyBytes>()
                .is_ok_and(|bytes| bytes.as_bytes().is_empty())
        });
        Self {
            altchars: options.altchars,
            padded: options.padded,
            strict_mode: options.strict_mode(),
            ignorechars_specified: options.ignorechars.is_some(),
            empty_ignorechars,
            canonical: options.canonical,
            python_315: options.ignorechars.is_none()
                && !options.canonical
                && options.altchars == Some(*b"-_")
                && python_at_least(py, (3, 15)),
        }
    }
}

impl DecodeStrategy {
    #[inline]
    pub(super) fn select(py: Python<'_>, options: DecodeOptions<'_, '_>) -> Self {
        Self::from_configuration(DecodeConfiguration::from_options(py, options))
    }

    fn from_configuration(configuration: DecodeConfiguration) -> Self {
        macro_rules! return_strategy {
            ($strategy:expr) => {
                $strategy
            };
        }
        select_decode_strategy!(configuration, return_strategy)
    }
}

pub(super) struct DecodePlan<'a, 'buffer, 'py> {
    input: &'a BytesLike<'buffer, 'py>,
    options: DecodeOptions<'a, 'py>,
}

impl<'a, 'buffer, 'py> DecodePlan<'a, 'buffer, 'py> {
    pub(super) fn new(input: &'a BytesLike<'buffer, 'py>, options: DecodeOptions<'a, 'py>) -> Self {
        Self { input, options }
    }

    pub(super) fn execute_allocating(self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let options = self.options;
        let mut skips_legacy_warning = false;
        let output =
            decode_plan_allocating_inner(py, self.input, options, &mut skips_legacy_warning)?;
        if !skips_legacy_warning {
            warn_legacy_altchars(
                py,
                self.input,
                options.altchars,
                options.ignorechars.is_some(),
                options.strict_mode(),
            )?;
        }
        Ok(output)
    }

    pub(super) fn execute_into(
        self,
        py: Python<'py>,
        output: &Bound<'py, PyByteArray>,
    ) -> PyResult<usize> {
        // Every decoder attempt and the warning scan must observe the same input.
        if let Some(input) = self.input.snapshot_for_output(output)? {
            let input = BytesLike::OwnedVec(input);
            return DecodePlan::new(&input, self.options).execute_into(py, output);
        }
        let options = self.options;
        let mut skips_legacy_warning = false;
        let written =
            decode_plan_into_inner(py, self.input, output, options, &mut skips_legacy_warning)?;
        if !skips_legacy_warning {
            warn_legacy_altchars(
                py,
                self.input,
                options.altchars,
                options.ignorechars.is_some(),
                options.strict_mode(),
            )?;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodeAttempt, DecodeConfiguration, DecodeOptions, DecodeStrategy};

    #[test]
    fn decode_options_preserve_validation_sentinel() {
        let implicit = DecodeOptions::new(None, None, true, None, false);
        let lenient = DecodeOptions::new(None, Some(false), true, None, false);
        let strict = DecodeOptions::new(None, Some(true), true, None, false);

        assert!(!implicit.strict_mode());
        assert!(!lenient.strict_mode());
        assert!(strict.strict_mode());
        assert_eq!(implicit.validate, None);
    }

    #[test]
    fn fixed_decode_options_match_public_helpers() {
        let standard = DecodeOptions::standard();
        let urlsafe = DecodeOptions::urlsafe(false);

        assert_eq!(standard.altchars, None);
        assert!(standard.padded);
        assert_eq!(urlsafe.altchars, Some(*b"-_"));
        assert!(!urlsafe.padded);
    }

    fn attempts(strategy: DecodeStrategy) -> Vec<DecodeAttempt> {
        let mut attempts = Vec::new();
        macro_rules! execute_attempt {
            ($attempt:ident) => {
                attempts.push(DecodeAttempt::$attempt);
            };
        }
        execute_decode_strategy!(strategy, execute_attempt);
        attempts
    }

    #[test]
    fn strategy_selection_covers_each_ordered_router() {
        let strict = DecodeStrategy::from_configuration(DecodeConfiguration {
            padded: true,
            strict_mode: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(strict),
            vec![DecodeAttempt::Strict, DecodeAttempt::Binascii]
        );

        let strict_urlsafe = DecodeStrategy::from_configuration(DecodeConfiguration {
            altchars: Some(*b"-_"),
            strict_mode: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(strict_urlsafe),
            vec![
                DecodeAttempt::Urlsafe315,
                DecodeAttempt::Unpadded,
                DecodeAttempt::Binascii,
            ]
        );

        let lenient = DecodeStrategy::from_configuration(DecodeConfiguration {
            padded: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(lenient),
            vec![
                DecodeAttempt::StrictProbe,
                DecodeAttempt::MimeWhitespace,
                DecodeAttempt::Lenient,
                DecodeAttempt::Binascii,
            ]
        );

        let lenient_urlsafe = DecodeStrategy::from_configuration(DecodeConfiguration {
            altchars: Some(*b"-_"),
            padded: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(lenient_urlsafe),
            vec![
                DecodeAttempt::Urlsafe315,
                DecodeAttempt::StrictProbe,
                DecodeAttempt::MimeWhitespace,
                DecodeAttempt::Lenient,
                DecodeAttempt::Binascii,
            ]
        );

        let custom_unpadded = DecodeStrategy::from_configuration(DecodeConfiguration {
            altchars: Some(*b"@#"),
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(custom_unpadded),
            vec![
                DecodeAttempt::Unpadded,
                DecodeAttempt::Lenient,
                DecodeAttempt::Binascii,
            ]
        );

        let canonical_padded = DecodeStrategy::from_configuration(DecodeConfiguration {
            padded: true,
            strict_mode: true,
            canonical: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(canonical_padded),
            vec![DecodeAttempt::StandardStrict, DecodeAttempt::Advanced,]
        );

        let canonical_unpadded = DecodeStrategy::from_configuration(DecodeConfiguration {
            strict_mode: true,
            canonical: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(canonical_unpadded),
            vec![DecodeAttempt::CanonicalUnpadded, DecodeAttempt::Advanced,]
        );

        let empty_ignorechars = DecodeStrategy::from_configuration(DecodeConfiguration {
            padded: true,
            strict_mode: true,
            ignorechars_specified: true,
            empty_ignorechars: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(
            attempts(empty_ignorechars),
            vec![DecodeAttempt::StandardStrict, DecodeAttempt::Advanced,]
        );

        let advanced = DecodeStrategy::from_configuration(DecodeConfiguration {
            altchars: Some(*b"@#"),
            padded: true,
            ignorechars_specified: true,
            python_315: true,
            ..DecodeConfiguration::default()
        });
        assert_eq!(attempts(advanced), vec![DecodeAttempt::Advanced]);
    }
}
