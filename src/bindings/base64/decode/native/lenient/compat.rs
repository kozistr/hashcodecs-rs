use pyo3::prelude::*;

pub(in crate::bindings::base64::decode) fn continues_after_padding(py: Python<'_>) -> bool {
    let version = py.version_info();
    version_continues_after_padding(version.major, version.minor, version.patch)
}

pub(in crate::bindings::base64::decode::native) fn version_continues_after_padding(
    major: u8,
    minor: u8,
    patch: u8,
) -> bool {
    match (major, minor) {
        (3, 13) => patch >= 13,
        (3, 14) => patch >= 4,
        (major, minor) => (major, minor) >= (3, 15),
    }
}
