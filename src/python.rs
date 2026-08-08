use core::slice;

use pyo3::exceptions::{PyAssertionError, PyBufferError, PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyModule, PyString, PyTuple, PyType};

use crate::{
    b64decode as decode_standard,
    base64::{
        DecodeAlphabet, decode_layout, decode_to_slice_with_layout_and_alphabet, encode_to_slice,
        encoded_len,
    },
    murmur3_x64_128, murmur3_x86_32, murmur3_x86_128,
};

const DETACH_THRESHOLD: usize = 64 * 1024;

enum BytesLike<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl BytesLike<'_> {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

fn bytes_like<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a>> {
    if value.is_instance_of::<PyList>() || value.is_instance_of::<PyTuple>() {
        return Err(PyTypeError::new_err(format!(
            "{argument} must be a bytes-like object"
        )));
    }
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(BytesLike::Borrowed(bytes.as_bytes()));
    }
    let builtins = py.import("builtins")?;
    let memoryview = builtins
        .getattr("memoryview")?
        .call1((value,))
        .map_err(|_| PyTypeError::new_err(format!("{argument} must be a bytes-like object")))?;
    let bytes = memoryview.call_method0("tobytes")?;
    let bytes = bytes
        .cast::<PyBytes>()
        .map_err(|_| PyTypeError::new_err("memoryview.tobytes() did not return bytes"))?;
    Ok(BytesLike::Owned(bytes.as_bytes().to_vec()))
}

fn contiguous_bytes_like<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a>> {
    if let Ok(bytes) = value.cast::<PyBytes>() {
        return Ok(BytesLike::Borrowed(bytes.as_bytes()));
    }
    let memoryview = py
        .import("builtins")?
        .getattr("memoryview")?
        .call1((value,))
        .map_err(|_| PyTypeError::new_err(format!("{argument} must be a bytes-like object")))?;
    if !memoryview.getattr("c_contiguous")?.is_truthy()? {
        return Err(PyBufferError::new_err(
            "memoryview: underlying buffer is not C-contiguous",
        ));
    }
    let bytes = memoryview.call_method0("tobytes")?;
    let bytes = bytes
        .cast::<PyBytes>()
        .map_err(|_| PyTypeError::new_err("memoryview.tobytes() did not return bytes"))?;
    Ok(BytesLike::Owned(bytes.as_bytes().to_vec()))
}

fn ascii_or_bytes<'a, 'py>(
    py: Python<'py>,
    value: &'a Bound<'py, PyAny>,
    argument: &str,
) -> PyResult<BytesLike<'a>> {
    if let Ok(text) = value.cast::<PyString>() {
        let text = text.to_str().map_err(|_| {
            PyValueError::new_err(format!("{argument} must contain only ASCII characters"))
        })?;
        if !text.is_ascii() {
            return Err(PyValueError::new_err(format!(
                "{argument} must contain only ASCII characters"
            )));
        }
        return Ok(BytesLike::Owned(text.as_bytes().to_vec()));
    }
    bytes_like(py, value, argument)
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
    let bytes = bytes.as_bytes();
    if bytes.len() != 2 {
        return Err(PyAssertionError::new_err(
            "altchars must be a bytes-like object or ASCII string of length 2",
        ));
    }
    Ok(Some([bytes[0], bytes[1]]))
}

fn pybytes_with_len<'py>(
    py: Python<'py>,
    length: usize,
    init: impl FnOnce(&mut [u8]) -> PyResult<()>,
) -> PyResult<Bound<'py, PyBytes>> {
    let length = ffi::Py_ssize_t::try_from(length)
        .map_err(|_| PyMemoryError::new_err("Base64 output is too large"))?;
    unsafe {
        let raw = ffi::PyBytes_FromStringAndSize(core::ptr::null(), length);
        let bytes: Bound<'py, PyBytes> =
            Bound::from_owned_ptr_or_err(py, raw)?.cast_into_unchecked();
        let buffer = ffi::PyBytes_AsString(raw).cast::<u8>();
        debug_assert!(!buffer.is_null());

        // The object is never exposed until the initializer has written every byte.
        init(slice::from_raw_parts_mut(buffer, length as usize)).map(|()| bytes)
    }
}

fn encode_with_altchars<'py>(
    py: Python<'py>,
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    pybytes_with_len(py, encoded_len(input.len()), |output| {
        let encode = || {
            let urlsafe = altchars == Some(*b"-_");
            encode_to_slice(input, output, urlsafe);
            if let Some([plus, slash]) = altchars.filter(|_| !urlsafe) {
                for byte in output {
                    if *byte == b'+' {
                        *byte = plus;
                    } else if *byte == b'/' {
                        *byte = slash;
                    }
                }
            }
        };
        if input.len() >= DETACH_THRESHOLD {
            py.detach(encode);
        } else {
            encode();
        }
        Ok(())
    })
}

fn decode_strict<'py>(
    py: Python<'py>,
    input: &[u8],
    alphabet: DecodeAlphabet,
) -> PyResult<Bound<'py, PyBytes>> {
    let layout = decode_layout(input).map_err(|_| decoding_error(py, "Incorrect padding"))?;
    pybytes_with_len(py, layout.output_len, |output| {
        let mut decode =
            || decode_to_slice_with_layout_and_alphabet(input, output, layout, alphabet);
        let result = if input.len() >= DETACH_THRESHOLD {
            py.detach(decode)
        } else {
            decode()
        };
        result.map_err(|_| decoding_error(py, "Only base64 data is allowed"))
    })
}

fn decoding_error(py: Python<'_>, message: &'static str) -> PyErr {
    match py
        .import("binascii")
        .and_then(|module| module.getattr("Error"))
        .and_then(|value| value.cast_into::<PyType>().map_err(Into::into))
    {
        Ok(error_type) => PyErr::from_type(error_type, (message,)),
        Err(error) => error,
    }
}

fn translate_altchars(input: &[u8], [plus, slash]: [u8; 2]) -> Vec<u8> {
    input
        .iter()
        .map(|&byte| {
            if byte == slash {
                b'/'
            } else if byte == plus {
                b'+'
            } else {
                byte
            }
        })
        .collect()
}

fn decode_strict_with_altchars<'py>(
    py: Python<'py>,
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyBytes>> {
    match altchars {
        None => decode_strict(py, input, DecodeAlphabet::Standard),
        Some([b'-', b'_']) => decode_strict(py, input, DecodeAlphabet::Mixed),
        Some(altchars) => decode_strict(
            py,
            &translate_altchars(input, altchars),
            DecodeAlphabet::Standard,
        ),
    }
}

fn decode_lenient(py: Python<'_>, input: &[u8], altchars: Option<[u8; 2]>) -> PyResult<Vec<u8>> {
    let mut normalized = Vec::with_capacity(input.len());
    let mut pending_padding = false;
    for &byte in input {
        let byte = match altchars {
            Some([_, slash]) if byte == slash => b'/',
            Some([plus, _]) if byte == plus => b'+',
            _ => byte,
        };
        if byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/') {
            pending_padding = false;
            normalized.push(byte);
        } else if byte == b'=' {
            match normalized.len() % 4 {
                2 if pending_padding => {
                    normalized.extend_from_slice(b"==");
                    pending_padding = false;
                    break;
                }
                2 => pending_padding = true,
                3 => {
                    normalized.push(b'=');
                    break;
                }
                _ => pending_padding = false,
            }
        }
    }
    if pending_padding {
        normalized.push(b'=');
    }
    decode_standard(&normalized).map_err(|_| decoding_error(py, "Incorrect padding"))
}

#[pyfunction(signature = (s, altchars=None))]
fn b64encode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = contiguous_bytes_like(py, s, "s")?;
    encode_with_altchars(py, input.as_bytes(), parse_altchars(py, altchars, false)?)
}

#[pyfunction(signature = (s, altchars=None, validate=false))]
fn b64decode<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = ascii_or_bytes(py, s, "s")?;
    let altchars = parse_altchars(py, altchars, true)?;
    if validate {
        return decode_strict_with_altchars(py, input.as_bytes(), altchars);
    }

    let direct = match altchars {
        None => Some(DecodeAlphabet::Standard),
        Some([b'-', b'_']) => Some(DecodeAlphabet::Mixed),
        Some(_) => None,
    };
    if let Some(alphabet) = direct {
        if let Ok(output) = decode_strict(py, input.as_bytes(), alphabet) {
            return Ok(output);
        }
    }
    let output = decode_lenient(py, input.as_bytes(), altchars)?;
    Ok(PyBytes::new(py, &output))
}

#[pyfunction(signature = (s, seed=0))]
fn murmur3_32(py: Python<'_>, s: &Bound<'_, PyAny>, seed: u32) -> PyResult<u32> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x86_32(input.as_bytes(), seed);
    Ok(if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    })
}

#[pyfunction(signature = (s, seed=0))]
fn murmur3_x86_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x86_128(input.as_bytes(), seed);
    let words = if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    };
    let mut digest = [0u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    Ok(PyBytes::new(py, &digest))
}

#[pyfunction(signature = (s, seed=0))]
fn murmur3_x64_128_digest<'py>(
    py: Python<'py>,
    s: &Bound<'py, PyAny>,
    seed: u32,
) -> PyResult<Bound<'py, PyBytes>> {
    let input = bytes_like(py, s, "s")?;
    let hash = || murmur3_x64_128(input.as_bytes(), seed);
    let words = if input.as_bytes().len() >= DETACH_THRESHOLD {
        py.detach(hash)
    } else {
        hash()
    };
    let mut digest = [0u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    Ok(PyBytes::new(py, &digest))
}

#[pymodule(name = "_hashcodecs")]
fn python_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(b64encode, module)?)?;
    module.add_function(wrap_pyfunction!(b64decode, module)?)?;
    module.add_function(wrap_pyfunction!(murmur3_32, module)?)?;
    module.add_function(wrap_pyfunction!(murmur3_x86_128_digest, module)?)?;
    module.add_function(wrap_pyfunction!(murmur3_x64_128_digest, module)?)?;
    Ok(())
}
