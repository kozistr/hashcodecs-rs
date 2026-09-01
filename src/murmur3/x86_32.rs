use super::block_buffer::{BlockBuffer, FullBlocks};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::dispatch;
use super::primitives::{fmix32, read_u32_le};
use core::fmt;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

pub(super) const X86_32_C1: u32 = 0xcc9e_2d51;
pub(super) const X86_32_C2: u32 = 0x1b87_3593;

/// Stores incremental state for the canonical MurmurHash3 x86 32-bit algorithm.
///
/// Call `update` with chunks of any size. A clone provides an independent checkpoint.
/// The `digest` method does not consume or reset the state.
/// MurmurHash3 does not provide collision resistance or preimage resistance.
///
/// # Examples
///
///     use hashcodecs::murmur3::Murmur3X86Hasher32;
///
///     let mut hasher = Murmur3X86Hasher32::new(7);
///     hasher.update(b"hello");
///     let checkpoint = hasher.clone();
///     hasher.update(b" world");
///     assert_ne!(hasher.digest(), checkpoint.digest());
///
#[derive(Clone)]
pub struct Murmur3X86Hasher32 {
    hash: u32,
    tail: BlockBuffer<4>,
    length: u32,
}

impl fmt::Debug for Murmur3X86Hasher32 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Murmur3Hasher")
            .field("algorithm", &"x86_32")
            .field("total_length", &self.length)
            .field("buffered_length", &self.tail.len())
            .finish()
    }
}

impl Murmur3X86Hasher32 {
    /// Creates an empty x86 32-bit hasher with the specified seed.
    ///
    /// # Arguments
    ///
    /// * `seed` - Specifies the initial unsigned 32-bit hash seed.
    ///
    /// # Returns
    ///
    /// The function returns a hasher that can receive bytes through `update`.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::{Murmur3X86Hasher32, murmur3_x86_32};
    ///
    ///     let hasher = Murmur3X86Hasher32::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x86_32(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hash: seed,
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
    ///     use hashcodecs::murmur3::{Murmur3X86Hasher32, murmur3_x86_32};
    ///
    ///     let mut hasher = Murmur3X86Hasher32::new(7);
    ///     hasher.update(b"hel");
    ///     hasher.update(b"lo");
    ///     assert_eq!(hasher.digest(), murmur3_x86_32(b"hello", 7));
    ///
    #[inline]
    pub fn update(&mut self, input: &[u8]) {
        self.length = self.length.wrapping_add(input.len() as u32);
        let hash = &mut self.hash;
        self.tail.consume(input, |blocks| {
            mix_x86_32_body(blocks, hash);
        });
    }

    /// Computes the current 32-bit digest without consuming the state.
    ///
    /// You can add more data after this call.
    ///
    /// # Returns
    ///
    /// The method returns the canonical unsigned x86 32-bit MurmurHash3 value.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::Murmur3X86Hasher32;
    ///
    ///     let mut hasher = Murmur3X86Hasher32::default();
    ///     hasher.update(b"hello");
    ///     assert_eq!(hasher.digest(), 0x248b_fa47);
    ///     assert_eq!(hasher.digest(), 0x248b_fa47);
    ///
    #[inline]
    pub fn digest(&self) -> u32 {
        finish_x86_32_tail(self.tail.remaining(), self.hash, self.length)
    }
}

impl Default for Murmur3X86Hasher32 {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Computes the canonical MurmurHash3 x86 32-bit hash in one call.
///
/// MurmurHash3 does not provide cryptographic security.
///
/// # Arguments
///
/// * `input` - Contains the bytes to hash.
/// * `seed` - Specifies the initial unsigned 32-bit seed.
///
/// # Returns
///
/// The function returns the canonical unsigned 32-bit result from the original x86 algorithm.
///
/// # Examples
///
///     use hashcodecs::murmur3::murmur3_x86_32;
///
///     assert_eq!(murmur3_x86_32(b"hello", 0), 0x248b_fa47);
///     assert_ne!(murmur3_x86_32(b"hello", 1), murmur3_x86_32(b"hello", 0));
///
#[inline]
pub fn murmur3_x86_32(input: &[u8], seed: u32) -> u32 {
    let (blocks, tail) = FullBlocks::<4>::split(input);
    let mut hash = seed;
    mix_x86_32_body(blocks, &mut hash);
    finish_x86_32_tail(tail, hash, input.len() as u32)
}

#[inline]
pub(super) fn mix_x86_32_body(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if blocks.byte_len() < dispatch::X86_32_SSE41_MIN {
            mix_x86_32_body_scalar(blocks, hash);
            return;
        }
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::select_x86_32_backend(blocks.byte_len(), capabilities);
        mix_x86_32_body_with_backend(blocks, hash, selected);
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    mix_x86_32_body_scalar(blocks, hash);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn mix_x86_32_body_with_backend(
    blocks: FullBlocks<'_, 4>,
    hash: &mut u32,
    backend: dispatch::Backend,
) {
    unsafe { x86::mix_x86_32_body(blocks, hash, backend) };
}

#[inline]
#[cfg(test)]
pub(super) fn murmur3_x86_32_scalar(input: &[u8], seed: u32) -> u32 {
    let (blocks, tail) = FullBlocks::<4>::split(input);
    let mut hash = seed;
    mix_x86_32_body_scalar(blocks, &mut hash);
    finish_x86_32_tail(tail, hash, input.len() as u32)
}

#[inline]
pub(super) fn mix_x86_32_body_scalar(blocks: FullBlocks<'_, 4>, hash: &mut u32) {
    let input = blocks.as_bytes();
    let mut offset = 0;
    while offset < input.len() {
        let block = read_u32_le(input, offset)
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
        *hash = mix_x86_32_hash(*hash, block);
        offset += 4;
    }
}

#[inline(always)]
pub(super) fn mix_x86_32_hash(mut hash: u32, block: u32) -> u32 {
    hash ^= block;
    hash.rotate_left(13)
        .wrapping_mul(5)
        .wrapping_add(0xe654_6b64)
}

#[inline(always)]
#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
pub(super) fn finish_x86_32(input: &[u8], hash: u32, offset: usize) -> u32 {
    finish_x86_32_tail(&input[offset..], hash, input.len() as u32)
}

#[inline(always)]
pub(super) fn finish_x86_32_tail(tail_bytes: &[u8], mut hash: u32, length: u32) -> u32 {
    debug_assert!(tail_bytes.len() < 4);
    let tail_len = tail_bytes.len();
    let mut tail = 0u32;
    if tail_len == 3 {
        tail ^= (tail_bytes[2] as u32) << 16;
    }
    if tail_len >= 2 {
        tail ^= (tail_bytes[1] as u32) << 8;
    }
    if tail_len != 0 {
        tail ^= tail_bytes[0] as u32;
        hash ^= tail
            .wrapping_mul(X86_32_C1)
            .rotate_left(15)
            .wrapping_mul(X86_32_C2);
    }

    fmix32(hash ^ length)
}
