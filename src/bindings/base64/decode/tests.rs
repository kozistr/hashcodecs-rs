use super::*;
use crate::bindings::buffer::with_bytearray;
use crate::bindings::compatibility::python_semantics;
use pyo3::exceptions::PyMemoryError;
use std::cell::Cell;

struct FailedStorage {
    attempts: Cell<usize>,
}

impl<'py> DecodeOutput<'py> for FailedStorage {
    type Value = ();
    const REUSABLE: bool = false;

    fn native(
        &self,
        _py: Python<'py>,
        _input: &BytesLike<'_, 'py>,
        _decoder: NativeDecoder<'_>,
        _prepared: &PreparedDecoder,
        _writes: ErrorWrites,
    ) -> PyResult<Result<(), Base64Error>> {
        self.attempts.set(self.attempts.get() + 1);
        Err(PyMemoryError::new_err("storage failed"))
    }

    fn lenient(
        &self,
        _py: Python<'py>,
        _input: &BytesLike<'_, 'py>,
        _prepared: &PreparedDecoder,
    ) -> PyResult<Result<(), Base64Error>> {
        panic!("storage errors must stop lenient retries")
    }

    fn store_fallback(&self, _bytes: Bound<'py, PyBytes>) -> PyResult<()> {
        panic!("storage errors must stop Python fallback")
    }
}

#[test]
fn propagate_storage_errors() {
    Python::initialize();
    Python::attach(|py| {
        let ignored = PyBytes::new(py, b"!");
        for (padded, canonical, ignored, encoded) in [
            (true, false, None, b"YWJj".as_slice()),
            (false, false, None, b"YQ"),
            (true, true, None, b"YWJj"),
            (false, true, None, b"YQ"),
            (true, false, Some(ignored.as_any()), b"Y!WJj"),
        ] {
            let decoder = PreparedDecoder::new(
                py,
                DecodePolicy::new(None, Some(false), padded, ignored, canonical),
            )
            .unwrap();
            let output = FailedStorage {
                attempts: Cell::new(0),
            };
            let error = decoder
                .execute(py, &BytesLike::OwnedVec(encoded.to_vec()), &output, None)
                .unwrap_err();
            assert!(error.is_instance_of::<PyMemoryError>(py));
            assert_eq!(error.to_string(), "MemoryError: storage failed");
            assert_eq!(output.attempts.get(), 1);
        }
    });
}

#[test]
fn snapshot_translated_input() {
    Python::initialize();
    Python::attach(|py| {
        let source = PyByteArray::new(py, b"@#8=");
        let (input, altchars) =
            prepare_translated_input(py, DecodeDataObject::Borrowed(source.as_any()), *b"@#")
                .unwrap();
        with_bytearray(&source, || unsafe { source.as_bytes_mut().fill(b'!') });
        assert_eq!(altchars, Some(*b"@#"));
        assert_eq!(
            input.as_bound().cast::<PyBytes>().unwrap().as_bytes(),
            b"@#8="
        );
    });
}

#[test]
fn snapshot_fallback_input() {
    Python::initialize();
    Python::attach(|py| {
        let source = PyByteArray::new(py, b"@#8=");
        let result = decode_with_binascii(
            py,
            python_semantics(py),
            &BytesLike::ByteArray(&source),
            Some(*b"@#"),
            Validation::Strict,
            Padding::Padded,
        )
        .unwrap();
        with_bytearray(&source, || unsafe { source.as_bytes_mut().fill(b'!') });
        assert_eq!(result.as_bytes(), b"\xfb\xff");
    });
}

#[test]
fn reject_invalid_alphabets() {
    Python::initialize();
    Python::attach(|py| {
        for length in [0, 63, 65] {
            let alphabet = PyBytes::new(py, &vec![b'A'; length]);
            let error = parse_decode_alphabet(alphabet.as_any()).err().unwrap();
            assert!(error.is_instance_of::<PyValueError>(py));
            assert_eq!(
                error.to_string(),
                "ValueError: alphabet must have length 64"
            );
        }
    });
}

#[test]
fn check_padding_bits() {
    for (input, canonical) in [
        (b"".as_slice(), true),
        (b"AA", true),
        (b"AB", false),
        (b"AAA", true),
        (b"AAB", false),
        (b"AAAA", true),
    ] {
        assert_eq!(canonical_padding(input), canonical);
    }
}
