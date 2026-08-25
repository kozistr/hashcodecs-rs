use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes};

use super::{
    decode_plan_allocating_inner, decode_plan_into_inner, python_at_least, try_decode_urlsafe_315,
    try_decode_urlsafe_315_into, warn_legacy_altchars,
};
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

pub(super) struct DecodePlan<'a, 'buffer, 'py> {
    input: &'a BytesLike<'buffer, 'py>,
    options: DecodeOptions<'a, 'py>,
}

pub(super) enum DecodeExecution<'a, 'py> {
    Allocate,
    Into(&'a Bound<'py, PyByteArray>),
}

pub(super) enum DecodeOutput<'py> {
    Bytes(Bound<'py, PyBytes>),
    Written(usize),
}

impl<'py> DecodeOutput<'py> {
    pub(super) fn into_bytes(self) -> Bound<'py, PyBytes> {
        match self {
            Self::Bytes(output) => output,
            Self::Written(_) => unreachable!("allocating decode returns bytes"),
        }
    }

    pub(super) fn into_written(self) -> usize {
        match self {
            Self::Written(written) => written,
            Self::Bytes(_) => unreachable!("decode-into returns a byte count"),
        }
    }
}

impl<'a, 'buffer, 'py> DecodePlan<'a, 'buffer, 'py> {
    pub(super) fn new(input: &'a BytesLike<'buffer, 'py>, options: DecodeOptions<'a, 'py>) -> Self {
        Self { input, options }
    }

    pub(super) fn execute(
        self,
        py: Python<'py>,
        execution: DecodeExecution<'a, 'py>,
    ) -> PyResult<DecodeOutput<'py>> {
        match execution {
            DecodeExecution::Allocate => self.execute_allocating(py).map(DecodeOutput::Bytes),
            DecodeExecution::Into(output) => {
                self.execute_into(py, output).map(DecodeOutput::Written)
            }
        }
    }

    fn execute_allocating(self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let options = self.options;
        if options.ignorechars.is_none()
            && !options.canonical
            && options.altchars == Some(*b"-_")
            && python_at_least(py, (3, 15))
            && let Some(output) =
                try_decode_urlsafe_315(py, self.input, options.strict_mode(), options.padded)?
        {
            // A successful strict URL-safe decode proves that no legacy `+` or
            // `/` characters were present, so no warning scan is necessary.
            return Ok(output);
        }
        let output = decode_plan_allocating_inner(py, self.input, options)?;
        warn_legacy_altchars(
            py,
            self.input,
            options.altchars,
            options.ignorechars.is_some(),
            options.strict_mode(),
        )?;
        Ok(output)
    }

    fn execute_into(self, py: Python<'py>, output: &Bound<'py, PyByteArray>) -> PyResult<usize> {
        let options = self.options;
        if options.ignorechars.is_none()
            && !options.canonical
            && options.altchars == Some(*b"-_")
            && python_at_least(py, (3, 15))
            && let Some(written) = try_decode_urlsafe_315_into(
                self.input,
                output,
                options.strict_mode(),
                options.padded,
            )?
        {
            // The strict URL-safe decoder rejects legacy standard-alphabet bytes.
            return Ok(written);
        }
        let written = decode_plan_into_inner(py, self.input, output, options)?;
        warn_legacy_altchars(
            py,
            self.input,
            options.altchars,
            options.ignorechars.is_some(),
            options.strict_mode(),
        )?;
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::DecodeOptions;

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
}
