//! CPython version rules and compatible argument validation.

use super::buffer::{ascii_or_bytes, contiguous_bytes_like};
use pyo3::exceptions::{PyAssertionError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::sync::OnceLock;

static PYTHON_SEMANTICS: OnceLock<PythonSemantics> = OnceLock::new();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PythonSemantics {
    version: (u8, u8, u8),
    binascii_api: BinasciiApi,
    pub(super) urlsafe_exclusive_alphabet: bool,
    pub(super) warns_legacy_altchars: bool,
    pub(super) continues_after_padding: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BinasciiApi {
    Legacy,
    StrictMode,
    Padding,
}

impl PythonSemantics {
    pub(super) fn from_version(version: (u8, u8, u8)) -> Self {
        let minor_version = (version.0, version.1);
        let binascii_api = if minor_version >= (3, 15) {
            BinasciiApi::Padding
        } else if minor_version >= (3, 11) {
            BinasciiApi::StrictMode
        } else {
            BinasciiApi::Legacy
        };
        let urlsafe_315 = minor_version >= (3, 15);
        let continues_after_padding = match version {
            (3, 13, patch) => patch >= 13,
            (3, 14, patch) => patch >= 4,
            (major, minor, _) => (major, minor) >= (3, 15),
        };

        Self {
            version,
            binascii_api,
            urlsafe_exclusive_alphabet: urlsafe_315,
            warns_legacy_altchars: minor_version >= (3, 15),
            continues_after_padding,
        }
    }

    fn at_least(self, version: (u8, u8)) -> bool {
        (self.version.0, self.version.1) >= version
    }

    pub(super) fn binascii_accepts_strict_mode(self) -> bool {
        self.binascii_api != BinasciiApi::Legacy
    }

    pub(super) fn binascii_accepts_padding(self) -> bool {
        self.binascii_api == BinasciiApi::Padding
    }
}

#[inline]
pub(super) fn python_at_least(py: Python<'_>, version: (u8, u8)) -> bool {
    python_semantics(py).at_least(version)
}

#[inline]
pub(super) fn python_semantics(py: Python<'_>) -> PythonSemantics {
    *PYTHON_SEMANTICS.get_or_init(|| {
        let version = py.version_info();
        PythonSemantics::from_version((version.major, version.minor, version.patch))
    })
}

pub(super) fn parse_altchars(
    py: Python<'_>,
    value: Option<&Bound<'_, PyAny>>,
    allow_text: bool,
) -> PyResult<Option<[u8; 2]>> {
    let Some(value) = value else {
        return Ok(None);
    };

    let bytes = if allow_text {
        ascii_or_bytes(py, value, "altchars")?
    } else {
        contiguous_bytes_like(value, "altchars")?
    };

    #[cfg(Py_GIL_DISABLED)]
    let bytes = bytes.into_stable()?;
    if bytes.len() != 2 {
        if python_at_least(py, (3, 15)) {
            let value = if allow_text {
                unsafe { bytes.with_bytes(|bytes| PyBytes::new(py, bytes).repr()) }?.to_string()
            } else {
                value.repr()?.to_string()
            };
            return Err(PyValueError::new_err(format!("invalid altchars: {value}",)));
        }
        return Err(PyAssertionError::new_err(
            "altchars must be a bytes-like object or ASCII string of length 2",
        ));
    }

    let altchars = unsafe { bytes.with_bytes(|bytes| [bytes[0], bytes[1]]) };

    Ok((altchars != *b"+/").then_some(altchars))
}
