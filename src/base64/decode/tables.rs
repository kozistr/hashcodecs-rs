//! Tables shared by the architecture-specific Base64 decoders.

pub(super) const STANDARD_OFFSETS: [u8; 16] =
    [0, 16, 19, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];
pub(super) const URLSAFE_OFFSETS: [u8; 16] =
    [0, 0, 17, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];

// Invalid high/low nibble pairs share a class bit. Valid pairs produce zero.
// The same class maps work with every SIMD decoder; only their vector loads
// differ by architecture.
pub(crate) const STANDARD_HIGH_CLASSES: [u8; 16] = [
    0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x08, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
];
pub(super) const STANDARD_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x2a, 0x2b, 0x2b, 0x2b, 0x2a,
];
pub(super) const URLSAFE_HIGH_CLASSES: [u8; 16] = [
    0x20, 0x20, 0x01, 0x02, 0x04, 0x08, 0x04, 0x10, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20, 0x20,
];
pub(super) const URLSAFE_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3b, 0x3b, 0x3a, 0x3b, 0x33,
];
pub(super) const MIXED_LOW_CLASSES: [u8; 16] = [
    0x25, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x21, 0x23, 0x3a, 0x3b, 0x3a, 0x3b, 0x32,
];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
const COMPLEMENTED_LOW_CLASSES: [[u8; 16]; 3] = {
    let classes = [STANDARD_LOW_CLASSES, URLSAFE_LOW_CLASSES, MIXED_LOW_CLASSES];
    let mut complemented = [[0; 16]; 3];
    let mut alphabet = 0;

    while alphabet < classes.len() {
        let mut index = 0;
        while index < classes[alphabet].len() {
            complemented[alphabet][index] = !classes[alphabet][index];
            index += 1;
        }
        alphabet += 1;
    }

    complemented
};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(crate) const STANDARD_LOW_CLASSES_COMPLEMENT: [u8; 16] = COMPLEMENTED_LOW_CLASSES[0];
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const URLSAFE_LOW_CLASSES_COMPLEMENT: [u8; 16] = COMPLEMENTED_LOW_CLASSES[1];
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const MIXED_LOW_CLASSES_COMPLEMENT: [u8; 16] = COMPLEMENTED_LOW_CLASSES[2];
#[cfg(target_arch = "aarch64")]
pub(super) const MIXED_OFFSETS: [u8; 16] =
    [0, 16, 19, 4, 191, 191, 185, 185, 17, 224, 0, 0, 0, 0, 0, 0];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const PACK_SHUFFLE: [u8; 16] = [
    2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 0xff, 0xff, 0xff, 0xff,
];
