//! Tables shared by the architecture-specific Base64 decoders.

pub(super) const STANDARD_OFFSETS: [u8; 16] =
    [0, 16, 19, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];
pub(super) const URLSAFE_OFFSETS: [u8; 16] =
    [0, 0, 17, 4, 191, 191, 185, 185, 0, 0, 0, 0, 0, 0, 0, 0];

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) const PACK_SHUFFLE: [u8; 16] = [
    2, 1, 0, 6, 5, 4, 10, 9, 8, 14, 13, 12, 0xff, 0xff, 0xff, 0xff,
];
