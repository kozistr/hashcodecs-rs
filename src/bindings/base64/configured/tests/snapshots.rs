use super::configured_decoder;
use crate::bindings::base64::configured::{decode_configured, decode_configured_into};
use crate::bindings::buffer::{BytesLike, with_bytearray};
use crate::bindings::compatibility::python_semantics;
use pyo3::prelude::*;
use pyo3::types::PyByteArray;

#[test]
fn snapshot_mutable_input() {
    Python::initialize();
    Python::attach(|py| {
        let source = PyByteArray::new(py, b"Y!WJj");
        let input = BytesLike::ByteArray(&source);
        let decoder = configured_decoder(b"!", true, true, false);
        let semantics = python_semantics(py);
        let allocated = decode_configured(py, &input, &decoder, semantics)
            .unwrap()
            .unwrap();
        assert_eq!(allocated.as_bytes(), b"abc");
        assert_eq!(
            decode_configured_into(py, &input, &source, &decoder, semantics).unwrap(),
            Ok(3)
        );
        with_bytearray(&source, || {
            assert_eq!(unsafe { source.as_bytes() }, b"abcJj")
        });
        assert_eq!(allocated.as_bytes(), b"abc");
    });
}
