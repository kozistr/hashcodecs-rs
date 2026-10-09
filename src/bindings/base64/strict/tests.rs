use super::*;
use crate::bindings::buffer::with_bytearray;

#[test]
fn snapshot_mutable_input() {
    Python::initialize();
    Python::attach(|py| {
        for (encoded, expected) in [(b"YWI=".as_slice(), b"ab".as_slice()), (b"YWI", b"ab")] {
            let source = PyByteArray::new(py, encoded);
            let input = BytesLike::ByteArray(&source);
            let result = if encoded.ends_with(b"=") {
                decode_strict(py, &input, DecodeAlphabet::Standard)
            } else {
                decode_unpadded(py, &input, DecodeAlphabet::Standard)
            }
            .unwrap()
            .unwrap();
            with_bytearray(&source, || unsafe { source.as_bytes_mut().fill(b'!') });
            assert_eq!(result.as_bytes(), expected);
        }
    });
}
