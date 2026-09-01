use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes};

use super::fallback::warn_legacy_altchars;
use super::output::{AllocatingExecutor, IntoExecutor};
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

const ADVANCED: &[DecodeAttempt] = &[DecodeAttempt::Advanced];
const STANDARD_STRICT_ADVANCED: &[DecodeAttempt] =
    &[DecodeAttempt::StandardStrict, DecodeAttempt::Advanced];
const CANONICAL_UNPADDED_ADVANCED: &[DecodeAttempt] =
    &[DecodeAttempt::CanonicalUnpadded, DecodeAttempt::Advanced];
const STRICT_PADDED: &[DecodeAttempt] = &[DecodeAttempt::Strict, DecodeAttempt::Binascii];
const STRICT_UNPADDED: &[DecodeAttempt] = &[DecodeAttempt::Unpadded, DecodeAttempt::Binascii];
const URLSAFE_315_STRICT_PADDED: &[DecodeAttempt] = &[
    DecodeAttempt::Urlsafe315,
    DecodeAttempt::Strict,
    DecodeAttempt::Binascii,
];
const URLSAFE_315_STRICT_UNPADDED: &[DecodeAttempt] = &[
    DecodeAttempt::Urlsafe315,
    DecodeAttempt::Unpadded,
    DecodeAttempt::Binascii,
];
const LENIENT_DIRECT_PADDED: &[DecodeAttempt] = &[
    DecodeAttempt::StrictProbe,
    DecodeAttempt::MimeWhitespace,
    DecodeAttempt::Lenient,
    DecodeAttempt::Binascii,
];
const LENIENT_DIRECT_UNPADDED: &[DecodeAttempt] = &[
    DecodeAttempt::StrictProbe,
    DecodeAttempt::Unpadded,
    DecodeAttempt::Lenient,
    DecodeAttempt::Binascii,
];
const URLSAFE_315_LENIENT_DIRECT_PADDED: &[DecodeAttempt] = &[
    DecodeAttempt::Urlsafe315,
    DecodeAttempt::StrictProbe,
    DecodeAttempt::MimeWhitespace,
    DecodeAttempt::Lenient,
    DecodeAttempt::Binascii,
];
const URLSAFE_315_LENIENT_DIRECT_UNPADDED: &[DecodeAttempt] = &[
    DecodeAttempt::Urlsafe315,
    DecodeAttempt::StrictProbe,
    DecodeAttempt::Unpadded,
    DecodeAttempt::Lenient,
    DecodeAttempt::Binascii,
];
const LENIENT_CUSTOM_PADDED: &[DecodeAttempt] = &[DecodeAttempt::Lenient, DecodeAttempt::Binascii];
const LENIENT_CUSTOM_UNPADDED: &[DecodeAttempt] = &[
    DecodeAttempt::Unpadded,
    DecodeAttempt::Lenient,
    DecodeAttempt::Binascii,
];

fn plan_attempts(configuration: DecodeConfiguration) -> &'static [DecodeAttempt] {
    let DecodeConfiguration {
        altchars,
        padded,
        strict_mode,
        ignorechars_specified,
        empty_ignorechars,
        canonical,
        python_315,
    } = configuration;
    let standard_strict = altchars.is_none()
        && padded
        && (!ignorechars_specified || empty_ignorechars)
        && (canonical || empty_ignorechars);
    if ignorechars_specified || canonical {
        return if standard_strict {
            STANDARD_STRICT_ADVANCED
        } else if !ignorechars_specified && canonical && !padded {
            CANONICAL_UNPADDED_ADVANCED
        } else {
            ADVANCED
        };
    }

    let urlsafe_315 = python_315 && altchars == Some(*b"-_");
    if strict_mode {
        return match (urlsafe_315, padded) {
            (false, true) => STRICT_PADDED,
            (false, false) => STRICT_UNPADDED,
            (true, true) => URLSAFE_315_STRICT_PADDED,
            (true, false) => URLSAFE_315_STRICT_UNPADDED,
        };
    }

    let direct = matches!(altchars, None | Some([b'-', b'_']));
    match (urlsafe_315, direct, padded) {
        (false, true, true) => LENIENT_DIRECT_PADDED,
        (false, true, false) => LENIENT_DIRECT_UNPADDED,
        (true, true, true) => URLSAFE_315_LENIENT_DIRECT_PADDED,
        (true, true, false) => URLSAFE_315_LENIENT_DIRECT_UNPADDED,
        (false, false, true) => LENIENT_CUSTOM_PADDED,
        (false, false, false) => LENIENT_CUSTOM_UNPADDED,
        (true, false, _) => unreachable!("Python 3.15 routing requires the URL-safe alphabet"),
    }
}

pub(super) struct DecodePlan<'a, 'buffer, 'py> {
    input: &'a BytesLike<'buffer, 'py>,
    options: DecodeOptions<'a, 'py>,
    attempts: &'static [DecodeAttempt],
}

impl<'a, 'buffer, 'py> DecodePlan<'a, 'buffer, 'py> {
    pub(super) fn new(
        py: Python<'py>,
        input: &'a BytesLike<'buffer, 'py>,
        options: DecodeOptions<'a, 'py>,
    ) -> Self {
        Self {
            input,
            options,
            attempts: plan_attempts(DecodeConfiguration::from_options(py, options)),
        }
    }

    pub(super) fn execute_allocating(self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let execution = AllocatingExecutor::execute(py, self.input, self.options, self.attempts)?;
        self.finish(py, execution.value, execution.skips_legacy_warning)
    }

    pub(super) fn execute_into(
        self,
        py: Python<'py>,
        output: &Bound<'py, PyByteArray>,
    ) -> PyResult<usize> {
        // Every decoder attempt and the warning scan must observe the same input.
        if let Some(input) = self.input.snapshot_for_output(output)? {
            let input = BytesLike::OwnedVec(input);
            return DecodePlan::new(py, &input, self.options).execute_into(py, output);
        }
        let execution = IntoExecutor::execute(py, self.input, output, self.options, self.attempts)?;
        self.finish(py, execution.value, execution.skips_legacy_warning)
    }

    fn finish<T>(self, py: Python<'_>, value: T, skips_legacy_warning: bool) -> PyResult<T> {
        if !skips_legacy_warning {
            warn_legacy_altchars(
                py,
                self.input,
                self.options.altchars,
                self.options.ignorechars.is_some(),
                self.options.strict_mode(),
            )?;
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodeAttempt, DecodeConfiguration, DecodeOptions, plan_attempts};

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

    #[test]
    fn planner_covers_each_ordered_route() {
        let cases = [
            (
                DecodeConfiguration {
                    padded: true,
                    strict_mode: true,
                    ..DecodeConfiguration::default()
                },
                &[DecodeAttempt::Strict, DecodeAttempt::Binascii][..],
            ),
            (
                DecodeConfiguration {
                    altchars: Some(*b"-_"),
                    strict_mode: true,
                    python_315: true,
                    ..DecodeConfiguration::default()
                },
                &[
                    DecodeAttempt::Urlsafe315,
                    DecodeAttempt::Unpadded,
                    DecodeAttempt::Binascii,
                ][..],
            ),
            (
                DecodeConfiguration {
                    padded: true,
                    ..DecodeConfiguration::default()
                },
                &[
                    DecodeAttempt::StrictProbe,
                    DecodeAttempt::MimeWhitespace,
                    DecodeAttempt::Lenient,
                    DecodeAttempt::Binascii,
                ][..],
            ),
            (
                DecodeConfiguration {
                    altchars: Some(*b"-_"),
                    padded: true,
                    python_315: true,
                    ..DecodeConfiguration::default()
                },
                &[
                    DecodeAttempt::Urlsafe315,
                    DecodeAttempt::StrictProbe,
                    DecodeAttempt::MimeWhitespace,
                    DecodeAttempt::Lenient,
                    DecodeAttempt::Binascii,
                ][..],
            ),
            (
                DecodeConfiguration {
                    altchars: Some(*b"@#"),
                    ..DecodeConfiguration::default()
                },
                &[
                    DecodeAttempt::Unpadded,
                    DecodeAttempt::Lenient,
                    DecodeAttempt::Binascii,
                ][..],
            ),
            (
                DecodeConfiguration {
                    padded: true,
                    canonical: true,
                    ..DecodeConfiguration::default()
                },
                &[DecodeAttempt::StandardStrict, DecodeAttempt::Advanced][..],
            ),
            (
                DecodeConfiguration {
                    canonical: true,
                    ..DecodeConfiguration::default()
                },
                &[DecodeAttempt::CanonicalUnpadded, DecodeAttempt::Advanced][..],
            ),
            (
                DecodeConfiguration {
                    padded: true,
                    ignorechars_specified: true,
                    empty_ignorechars: true,
                    ..DecodeConfiguration::default()
                },
                &[DecodeAttempt::StandardStrict, DecodeAttempt::Advanced][..],
            ),
            (
                DecodeConfiguration {
                    altchars: Some(*b"@#"),
                    padded: true,
                    ignorechars_specified: true,
                    ..DecodeConfiguration::default()
                },
                &[DecodeAttempt::Advanced][..],
            ),
        ];
        for (configuration, expected) in cases {
            assert_eq!(plan_attempts(configuration), expected);
        }
    }
}
