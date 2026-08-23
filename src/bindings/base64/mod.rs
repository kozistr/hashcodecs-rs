use std::collections::HashSet;
use std::ptr;
use std::sync::OnceLock;

use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyAssertionError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyInt, PyList};

use self::decode::{
    b64decode, b64decode_batch, b64decode_batch_into, b64decode_into, standard_b64decode,
    standard_b64decode_batch, standard_b64decode_batch_into, standard_b64decode_into,
    urlsafe_b64decode_315, urlsafe_b64decode_batch, urlsafe_b64decode_batch_into,
    urlsafe_b64decode_into_315, urlsafe_b64decode_into_pre_315, urlsafe_b64decode_pre_315,
};
use super::arguments::parse_raw_arguments;
use super::buffer::{BytesLike, ascii_or_bytes, contiguous_bytes_like, with_bytearray};
use super::objects::{bytearray_data, bytearray_size, bytes_data_mut, list_items};
use super::runtime::{METHOD_FLAGS, add_methods, return_function_result};

mod callbacks;
mod decode;
mod encode;
mod methods;

const STANDARD_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

static PYTHON_VERSION: OnceLock<(u8, u8)> = OnceLock::new();

#[inline]
pub(super) fn python_at_least(py: Python<'_>, version: (u8, u8)) -> bool {
    *PYTHON_VERSION.get_or_init(|| {
        let version_info = py.version_info();
        (version_info.major, version_info.minor)
    }) >= version
}

fn parse_altchars(
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
        contiguous_bytes_like(py, value, "altchars")?
    };
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

type PreparedAltchars = Result<Option<[u8; 2]>, PyErr>;

// Python 3.15 constructs a custom alphabet before consuming the input, but
// binascii validates its byte length afterward. The inner result preserves
// that otherwise-observable error ordering without sending valid calls back
// through Python's encoder.
fn prepare_b64encode_altchars(
    py: Python<'_>,
    value: &Bound<'_, PyAny>,
) -> PyResult<PreparedAltchars> {
    let length = value.len()?;
    if length != 2 {
        let value = value.repr()?.to_string();
        if python_at_least(py, (3, 15)) {
            return Err(PyValueError::new_err(format!("invalid altchars: {value}")));
        }
        return Err(PyAssertionError::new_err(value));
    }

    if python_at_least(py, (3, 15))
        && !PyBytes::is_exact_type_of(value)
        && !PyByteArray::is_exact_type_of(value)
    {
        let prefix = PyBytes::new(py, &STANDARD_ALPHABET[..62]);
        let alphabet = unsafe {
            Bound::from_owned_ptr_or_err(py, ffi::PyNumber_Add(prefix.as_ptr(), value.as_ptr()))?
        };
        let alphabet = match alphabet.cast_into::<PyBytes>() {
            Ok(alphabet) => alphabet,
            Err(error) => return Ok(Err(error.into())),
        };
        if alphabet.as_bytes().len() != STANDARD_ALPHABET.len() {
            return Ok(Err(PyValueError::new_err("alphabet must have length 64")));
        }
        let alphabet = alphabet.as_bytes();
        let altchars = [alphabet[62], alphabet[63]];
        return Ok(Ok((altchars != *b"+/").then_some(altchars)));
    }

    let bytes = contiguous_bytes_like(py, value, "altchars")?;
    if bytes.len() != 2 {
        let message = if python_at_least(py, (3, 15)) {
            "alphabet must have length 64"
        } else {
            "maketrans arguments must have same length"
        };
        return Err(PyValueError::new_err(message));
    }
    let altchars = unsafe { bytes.with_bytes(|bytes| [bytes[0], bytes[1]]) };
    Ok(Ok((altchars != *b"+/").then_some(altchars)))
}

/// Allocate an uninitialized Python `bytes` payload for direct initialization.
///
/// # Safety
/// If the returned Python object can escape, `init` must have initialized all
/// `length` bytes. An initialization error may leave bytes unwritten only when
/// the caller discards the object without reading its payload.
unsafe fn pybytes_with_len<'py, T>(
    py: Python<'py>,
    length: usize,
    init: impl FnOnce(*mut u8) -> T,
) -> PyResult<(Bound<'py, PyBytes>, T)> {
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
    unsafe {
        let raw = ffi::PyBytes_FromStringAndSize(core::ptr::null(), length);
        let bytes: Bound<'py, PyBytes> =
            Bound::from_owned_ptr_or_err(py, raw)?.cast_into_unchecked();
        let buffer = bytes_data_mut(raw);
        debug_assert!(!buffer.is_null());

        // CPython leaves the payload uninitialized when passed a null source.
        // Keep it behind a raw pointer until the initializer has written every
        // byte instead of creating a Rust `&mut [u8]` with invalid contents.
        let initialized = init(buffer);
        Ok((bytes, initialized))
    }
}

fn with_output_ptr<T>(
    output: &Bound<'_, PyByteArray>,
    required: usize,
    callback: impl FnOnce(*mut u8) -> T,
) -> PyResult<T> {
    with_bytearray(output, || {
        let provided = unsafe { bytearray_size(output.as_ptr()) };
        if provided < required {
            return Err(output_too_small(required, provided));
        }
        Ok(callback(unsafe { bytearray_data(output.as_ptr()) }))
    })
}

fn output_too_small(required: usize, provided: usize) -> PyErr {
    PyValueError::new_err(format!(
        "Base64 output requires {required} bytes but the destination has {provided}"
    ))
}

fn batch_results<T>(length: usize) -> PyResult<Vec<T>> {
    let mut results = Vec::new();
    results
        .try_reserve_exact(length)
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    Ok(results)
}

fn batch_outputs<'py>(
    items_length: usize,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Vec<Bound<'py, PyByteArray>>> {
    if outputs.len() != items_length {
        return Err(PyValueError::new_err(
            "items and outputs must have the same length",
        ));
    }

    let mut parsed = batch_results(outputs.len())?;
    let mut identities = HashSet::new();
    identities
        .try_reserve(outputs.len())
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    for (index, output) in list_items(outputs).into_iter().enumerate() {
        let output = output
            .cast_into::<PyByteArray>()
            .map_err(|_| PyTypeError::new_err(format!("outputs[{index}] must be a bytearray")))?;
        if !identities.insert(output.as_ptr()) {
            return Err(PyValueError::new_err(
                "outputs must contain distinct bytearrays",
            ));
        }
        parsed.push(output);
    }
    Ok(parsed)
}

fn encode_parsed<'py>(
    py: Python<'py>,
    input: &Bound<'py, PyAny>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = contiguous_bytes_like(py, input, "s")?;
    encode::encode(py, &input, altchars, padded, wrapcol)
}

fn encode_parsed_into(
    input: &BytesLike<'_, '_>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<[u8; 2]>,
    padded: bool,
    wrapcol: Option<usize>,
) -> PyResult<usize> {
    encode::encode_into(input, output, altchars, padded, wrapcol)
}

/// Encode with the standard Base64 alphabet.
pub(super) fn standard_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_parsed(py, s, None, true, None)
}

/// Encode with the standard Base64 alphabet into a reusable output.
pub(super) fn standard_b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_parsed_into(&input, output, None, true, None)
}

/// Encode with the URL-safe Base64 alphabet.
pub(super) fn urlsafe_b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    padded: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    encode_parsed(py, s, Some(*b"-_"), padded, None)
}

/// Encode with the URL-safe Base64 alphabet into a reusable output.
pub(super) fn urlsafe_b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    padded: bool,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_parsed_into(&input, output, Some(*b"-_"), padded, None)
}

pub(super) fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    padded: bool,
    wrapcol: i128,
) -> PyResult<Bound<'py, PyBytes>> {
    let Some(altchars) = altchars else {
        let input = contiguous_bytes_like(py, s, "s")?;
        let wrapcol = encode::normalize_wrapcol(wrapcol)?;
        return encode::encode(py, &input, None, padded, wrapcol);
    };
    let parse_altchars_first = python_at_least(py, (3, 15));
    let parsed_altchars = parse_altchars_first
        .then(|| prepare_b64encode_altchars(py, altchars))
        .transpose()?;
    let input = contiguous_bytes_like(py, s, "s")?;
    let altchars = if let Some(parsed_altchars) = parsed_altchars {
        parsed_altchars?
    } else {
        prepare_b64encode_altchars(py, altchars)??
    };
    let wrapcol = encode::normalize_wrapcol(wrapcol)?;
    encode::encode(py, &input, altchars, padded, wrapcol)
}

/// Encode each bytes-like item and return results in input order.
///
/// ``items`` must be a list. ``altchars`` applies to every item. Processing is
/// fail-fast: an error discards the partial result and is raised immediately.
/// Processing is single-threaded. Immutable items of at least 64 KiB release
/// the GIL independently; smaller and mutable items do not. Do not mutate
/// ``items`` concurrently while this function is running.
pub(super) fn b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_parsed(py, items, altchars)
}

fn b64encode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    let mut encoded = batch_results(items.len())?;
    for item in list_items(items) {
        encoded.push(encode_parsed(py, &item, altchars, true, None)?);
    }
    PyList::new(py, encoded)
}

pub(super) fn standard_b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_parsed(py, items, None)
}

pub(super) fn urlsafe_b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_parsed(py, items, Some(*b"-_"))
}

/// Encode each item into its matching reusable bytearray and return byte counts.
///
/// ``items`` and ``outputs`` must be equal-length lists, and destinations must
/// be distinct bytearrays. Each destination keeps its size; only its written
/// prefix is changed. Processing is fail-fast and non-transactional: an error
/// leaves earlier destinations modified. The GIL remains held because outputs
/// are mutable. Do not share backing storage across different item/output pairs.
pub(super) fn b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_into_parsed(py, items, outputs, altchars)
}

fn b64encode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut written = batch_results(items.len())?;
    for (item, output) in list_items(items).into_iter().zip(outputs.iter()) {
        let input = contiguous_bytes_like(py, &item, "s")?;
        written.push(encode_parsed_into(&input, output, altchars, true, None)?);
    }
    PyList::new(py, written)
}

pub(super) fn standard_b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_into_parsed(py, items, outputs, None)
}

pub(super) fn urlsafe_b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
) -> PyResult<Bound<'py, PyList>> {
    b64encode_batch_into_parsed(py, items, outputs, Some(*b"-_"))
}

pub(super) fn b64encode_into(
    py: Python<'_>,
    s: &Bound<'_, PyAny>,
    output: &Bound<'_, PyByteArray>,
    altchars: Option<&Bound<'_, PyAny>>,
    padded: bool,
    wrapcol: i128,
) -> PyResult<usize> {
    let input = contiguous_bytes_like(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, false)?;
    encode_parsed_into(
        &input,
        output,
        altchars,
        padded,
        encode::normalize_wrapcol(wrapcol)?,
    )
}

unsafe fn raw_argument<'a, 'py>(
    py: Python<'py>,
    value: &'a *mut ffi::PyObject,
) -> &'a Bound<'py, PyAny> {
    unsafe { Bound::ref_from_ptr(py, value) }
}

unsafe fn optional_argument<'a, 'py>(
    py: Python<'py>,
    value: &'a *mut ffi::PyObject,
) -> Option<&'a Bound<'py, PyAny>> {
    if value.is_null() || *value == unsafe { ffi::Py_None() } {
        None
    } else {
        Some(unsafe { raw_argument(py, value) })
    }
}

unsafe fn provided_argument<'a, 'py>(
    py: Python<'py>,
    value: &'a *mut ffi::PyObject,
) -> Option<&'a Bound<'py, PyAny>> {
    (!value.is_null()).then(|| unsafe { raw_argument(py, value) })
}

unsafe fn truthy_argument(
    py: Python<'_>,
    value: *mut ffi::PyObject,
    default: bool,
) -> PyResult<bool> {
    if value.is_null() {
        return Ok(default);
    }
    let truthy = unsafe { ffi::PyObject_IsTrue(value) };
    if truthy == -1 {
        Err(PyErr::fetch(py))
    } else {
        Ok(truthy != 0)
    }
}

fn return_bound<T: PyTypeInfo>(
    py: Python<'_>,
    result: PyResult<Bound<'_, T>>,
) -> *mut ffi::PyObject {
    unsafe { return_function_result(py, result.map(Bound::into_ptr)) }
}

fn return_usize(py: Python<'_>, result: PyResult<usize>) -> *mut ffi::PyObject {
    unsafe { return_function_result(py, result.map(|value| PyInt::new(py, value).into_ptr())) }
}

pub(super) use methods::add_to_module;

#[cfg(test)]
mod tests {
    use super::batch_results;

    #[test]
    fn oversized_batch_capacity_is_an_error() {
        assert!(batch_results::<u8>(usize::MAX).is_err());
    }
}
