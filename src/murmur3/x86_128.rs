use super::block_buffer::{BlockBuffer, FullBlocks};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::dispatch;
use super::primitives::{fmix32, read_u32_le};
use core::fmt;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

pub(super) const X86_128_C1: [u32; 4] = [0x239b_961b, 0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93];
pub(super) const X86_128_C2: [u32; 4] = [0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93, 0x239b_961b];

/// Stores incremental state for the canonical MurmurHash3 x86 128-bit algorithm.
///
/// Call `update` with chunks of any size. The function orders the four digest words like the reference implementation.
/// MurmurHash3 does not provide cryptographic security.
///
/// # Examples
///
///     use hashcodecs::murmur3::Murmur3X86Hasher128;
///
///     let mut hasher = Murmur3X86Hasher128::new(7);
///     hasher.update(b"hello");
///     let checkpoint = hasher.clone();
///     hasher.update(b" world");
///     assert_ne!(hasher.digest(), checkpoint.digest());
///
#[derive(Clone)]
pub struct Murmur3X86Hasher128 {
    hashes: [u32; 4],
    tail: BlockBuffer<16>,
    length: u32,
}

impl fmt::Debug for Murmur3X86Hasher128 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Murmur3Hasher")
            .field("algorithm", &"x86_128")
            .field("total_length", &self.length)
            .field("buffered_length", &self.tail.len())
            .finish()
    }
}

impl Murmur3X86Hasher128 {
    /// Creates an empty x86 128-bit hasher with the specified seed.
    ///
    /// # Arguments
    ///
    /// * `seed` - Specifies the initial unsigned 32-bit seed for all four lanes.
    ///
    /// # Returns
    ///
    /// The function returns a hasher that can receive bytes through `update`.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::{Murmur3X86Hasher128, murmur3_x86_128};
    ///
    ///     let hasher = Murmur3X86Hasher128::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x86_128(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hashes: [seed; 4],
            tail: BlockBuffer::new(),
            length: 0,
        }
    }

    /// Adds bytes to the hash state.
    ///
    /// Multiple `update` calls produce the same result as one call with the combined input.
    ///
    /// # Arguments
    ///
    /// * `input` - Contains the next message bytes.
    ///
    /// # Returns
    ///
    /// The method returns unit. The hasher can receive more input after this call.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::{Murmur3X86Hasher128, murmur3_x86_128};
    ///
    ///     let mut hasher = Murmur3X86Hasher128::new(7);
    ///     hasher.update(b"hel");
    ///     hasher.update(b"lo");
    ///     assert_eq!(hasher.digest(), murmur3_x86_128(b"hello", 7));
    ///
    #[inline]
    pub fn update(&mut self, input: &[u8]) {
        self.length = self.length.wrapping_add(input.len() as u32);
        let hashes = &mut self.hashes;
        self.tail.consume(input, |blocks| {
            mix_x86_128_body(blocks, hashes);
        });
    }

    /// Computes the current 128-bit digest without consuming the state.
    ///
    /// # Returns
    ///
    /// The method returns four 32-bit words in canonical low-to-high reference order.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::Murmur3X86Hasher128;
    ///
    ///     let mut hasher = Murmur3X86Hasher128::default();
    ///     hasher.update(b"hello");
    ///     let first = hasher.digest();
    ///     assert_eq!(first, hasher.digest());
    ///
    #[inline]
    pub fn digest(&self) -> [u32; 4] {
        finish_x86_128_tail(self.tail.remaining(), self.hashes, self.length)
    }
}

impl Default for Murmur3X86Hasher128 {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Computes the canonical MurmurHash3 x86 128-bit hash in one call.
///
/// # Arguments
///
/// * `input` - Contains the bytes to hash.
/// * `seed` - Specifies the initial unsigned 32-bit seed for all four lanes.
///
/// # Returns
///
/// The function returns four unsigned 32-bit words in canonical low-to-high reference order.
/// To create the Python API digest, concatenate the little-endian bytes from each word.
///
/// # Examples
///
///     use hashcodecs::murmur3::murmur3_x86_128;
///
///     let words = murmur3_x86_128(&[1, 2, 3], 0);
///     let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
///     assert_eq!(
///         bytes,
///         [
///             0xe1, 0x64, 0x01, 0xf6, 0x33, 0x42, 0x13, 0xb5,
///             0x33, 0x42, 0x13, 0xb5, 0x33, 0x42, 0x13, 0xb5,
///         ],
///     );
///
#[inline]
pub fn murmur3_x86_128(input: &[u8], seed: u32) -> [u32; 4] {
    let (blocks, tail) = FullBlocks::<16>::split(input);
    let mut hashes = [seed; 4];
    mix_x86_128_body(blocks, &mut hashes);
    finish_x86_128_tail(tail, hashes, input.len() as u32)
}

#[inline]
#[cfg(test)]
pub(super) fn finish_x86_128(input: &[u8], hashes: [u32; 4], offset: usize) -> [u32; 4] {
    finish_x86_128_tail(&input[offset..], hashes, input.len() as u32)
}

#[inline]
pub(super) fn finish_x86_128_tail(tail: &[u8], mut hashes: [u32; 4], length: u32) -> [u32; 4] {
    debug_assert!(tail.len() < 16);
    let mut blocks = [0u32; 4];
    for (index, byte) in tail.iter().copied().enumerate() {
        blocks[index / 4] |= (byte as u32) << ((index % 4) * 8);
    }
    if tail.len() > 12 {
        hashes[3] ^= blocks[3]
            .wrapping_mul(X86_128_C1[3])
            .rotate_left(18)
            .wrapping_mul(X86_128_C2[3]);
    }
    if tail.len() > 8 {
        hashes[2] ^= blocks[2]
            .wrapping_mul(X86_128_C1[2])
            .rotate_left(17)
            .wrapping_mul(X86_128_C2[2]);
    }
    if tail.len() > 4 {
        hashes[1] ^= blocks[1]
            .wrapping_mul(X86_128_C1[1])
            .rotate_left(16)
            .wrapping_mul(X86_128_C2[1]);
    }
    if !tail.is_empty() {
        hashes[0] ^= blocks[0]
            .wrapping_mul(X86_128_C1[0])
            .rotate_left(15)
            .wrapping_mul(X86_128_C2[0]);
    }

    finalize_x86_128(hashes, length)
}

#[inline]
pub(super) fn mix_x86_128_body(blocks: FullBlocks<'_, 16>, hashes: &mut [u32; 4]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if blocks.byte_len() < dispatch::X86_128_AVX2_MIN {
            mix_x86_128_body_scalar(blocks, hashes);
            return;
        }
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::select_x86_128_backend(blocks.byte_len(), capabilities);
        mix_x86_128_body_with_backend(blocks, hashes, selected);
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    mix_x86_128_body_scalar(blocks, hashes);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn mix_x86_128_body_with_backend(
    blocks: FullBlocks<'_, 16>,
    hashes: &mut [u32; 4],
    backend: dispatch::Backend,
) {
    unsafe { x86::mix_x86_128_body(blocks, hashes, backend) };
}

#[inline]
pub(super) fn mix_x86_128_body_scalar(blocks: FullBlocks<'_, 16>, hashes: &mut [u32; 4]) {
    const ROTATE_K: [u32; 4] = [15, 16, 17, 18];

    let input = blocks.as_bytes();
    let mut offset = 0;
    while offset < input.len() {
        let block1 = read_u32_le(input, offset)
            .wrapping_mul(X86_128_C1[0])
            .rotate_left(ROTATE_K[0])
            .wrapping_mul(X86_128_C2[0]);
        let block2 = read_u32_le(input, offset + 4)
            .wrapping_mul(X86_128_C1[1])
            .rotate_left(ROTATE_K[1])
            .wrapping_mul(X86_128_C2[1]);
        let block3 = read_u32_le(input, offset + 8)
            .wrapping_mul(X86_128_C1[2])
            .rotate_left(ROTATE_K[2])
            .wrapping_mul(X86_128_C2[2]);
        let block4 = read_u32_le(input, offset + 12)
            .wrapping_mul(X86_128_C1[3])
            .rotate_left(ROTATE_K[3])
            .wrapping_mul(X86_128_C2[3]);
        mix_x86_128_hashes(hashes, block1, block2, block3, block4);
        offset += 16;
    }
}

#[inline(always)]
pub(super) fn mix_x86_128_hashes(
    hashes: &mut [u32; 4],
    block1: u32,
    block2: u32,
    block3: u32,
    block4: u32,
) {
    const ROTATE_H: [u32; 4] = [19, 17, 15, 13];
    const ADD: [u32; 4] = [0x561c_cd1b, 0x0bca_a747, 0x96cd_1c35, 0x32ac_3b17];

    hashes[0] ^= block1;
    hashes[0] = hashes[0]
        .rotate_left(ROTATE_H[0])
        .wrapping_add(hashes[1])
        .wrapping_mul(5)
        .wrapping_add(ADD[0]);
    hashes[1] ^= block2;
    hashes[1] = hashes[1]
        .rotate_left(ROTATE_H[1])
        .wrapping_add(hashes[2])
        .wrapping_mul(5)
        .wrapping_add(ADD[1]);
    hashes[2] ^= block3;
    hashes[2] = hashes[2]
        .rotate_left(ROTATE_H[2])
        .wrapping_add(hashes[3])
        .wrapping_mul(5)
        .wrapping_add(ADD[2]);
    hashes[3] ^= block4;
    hashes[3] = hashes[3]
        .rotate_left(ROTATE_H[3])
        .wrapping_add(hashes[0])
        .wrapping_mul(5)
        .wrapping_add(ADD[3]);
}

#[inline]
pub(super) fn finalize_x86_128(mut hashes: [u32; 4], length: u32) -> [u32; 4] {
    for hash in &mut hashes {
        *hash ^= length;
    }
    hashes[0] = hashes[0]
        .wrapping_add(hashes[1])
        .wrapping_add(hashes[2])
        .wrapping_add(hashes[3]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[2] = hashes[2].wrapping_add(hashes[0]);
    hashes[3] = hashes[3].wrapping_add(hashes[0]);
    for hash in &mut hashes {
        *hash = fmix32(*hash);
    }
    hashes[0] = hashes[0]
        .wrapping_add(hashes[1])
        .wrapping_add(hashes[2])
        .wrapping_add(hashes[3]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[2] = hashes[2].wrapping_add(hashes[0]);
    hashes[3] = hashes[3].wrapping_add(hashes[0]);
    hashes
}
