use pyo3::prelude::*;
use pyo3::types::PyString;

pub(super) fn x86_128_digest(words: [u32; 4]) -> [u8; 16] {
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

pub(super) fn x64_128_digest(words: [u64; 2]) -> [u8; 16] {
    let mut digest = [0_u8; 16];
    for (index, word) in words.iter().enumerate() {
        digest[index * 8..index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    digest
}

pub(super) fn hex_digest<'py>(py: Python<'py>, bytes: &[u8]) -> Bound<'py, PyString> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = [0_u8; 32];
    assert!(
        bytes.len() <= output.len() / 2,
        "Murmur3 digest exceeds 128 bits"
    );
    let output_len = bytes.len() * 2;
    for (index, &byte) in bytes.iter().enumerate() {
        output[index * 2] = HEX[(byte >> 4) as usize];
        output[index * 2 + 1] = HEX[(byte & 0x0f) as usize];
    }

    // Every byte comes from the lowercase ASCII table above.
    PyString::new(py, unsafe {
        str::from_utf8_unchecked(&output[..output_len])
    })
}
