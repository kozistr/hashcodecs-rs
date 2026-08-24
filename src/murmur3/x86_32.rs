#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::dispatch;
use super::incremental::BlockBuffer;
use super::primitives::{fmix32, read_u32_le};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::x86;

pub(super) const X86_32_C1: u32 = 0xcc9e_2d51;
pub(super) const X86_32_C2: u32 = 0x1b87_3593;

/// Incremental state for the canonical MurmurHash3 x86 32-bit algorithm.
///
/// Data can be supplied in any chunk sizes. Cloning the value creates an
/// independent checkpoint, and digest does not consume or reset the state.
/// MurmurHash3 is non-cryptographic and must not be used where collision or
/// preimage resistance is required.
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
#[derive(Clone, Debug)]
pub struct Murmur3X86Hasher32 {
    hash: u32,
    tail: BlockBuffer<4>,
    length: u32,
}

impl Murmur3X86Hasher32 {
    /// Creates an empty x86 32-bit hasher with the supplied seed.
    ///
    /// # Arguments
    ///
    /// * seed - The initial unsigned 32-bit hash seed.
    ///
    /// # Returns
    ///
    /// A hasher ready to receive bytes through update.
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

    /// Appends bytes to the hash state.
    ///
    /// Calling update multiple times is equivalent to hashing the concatenated
    /// input in one call.
    ///
    /// # Arguments
    ///
    /// * input - The next bytes in the message.
    ///
    /// # Returns
    ///
    /// This method returns unit and leaves the hasher ready for more input.
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
    /// More data may be appended after this call.
    ///
    /// # Returns
    ///
    /// The canonical unsigned x86 32-bit MurmurHash3 value.
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
/// MurmurHash3 is designed for fast hash tables and data processing, not for
/// cryptographic security.
///
/// # Arguments
///
/// * key - The bytes to hash.
/// * seed - The initial unsigned 32-bit seed.
///
/// # Returns
///
/// The canonical unsigned 32-bit result from the original x86 algorithm.
///
/// # Examples
///
///     use hashcodecs::murmur3::murmur3_x86_32;
///
///     assert_eq!(murmur3_x86_32(b"hello", 0), 0x248b_fa47);
///     assert_ne!(murmur3_x86_32(b"hello", 1), murmur3_x86_32(b"hello", 0));
///
#[inline]
pub fn murmur3_x86_32(key: &[u8], seed: u32) -> u32 {
    let block_end = key.len() & !3;
    let mut hash = seed;
    mix_x86_32_body(&key[..block_end], &mut hash);
    finish_x86_32(key, hash, block_end)
}

#[inline]
pub(super) fn mix_x86_32_body(key: &[u8], hash: &mut u32) {
    debug_assert!(key.len().is_multiple_of(4));
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::x86_32(key.len(), capabilities);
        if unsafe { x86::try_mix_x86_32_body(key, hash, selected) } {
            return;
        }
    }
    mix_x86_32_body_scalar(key, hash);
}

#[inline]
#[cfg(test)]
pub(super) fn murmur3_x86_32_scalar(key: &[u8], seed: u32) -> u32 {
    let mut hash = seed;
    let block_end = key.len() & !3;
    mix_x86_32_body_scalar(&key[..block_end], &mut hash);
    finish_x86_32(key, hash, block_end)
}

#[inline]
pub(super) fn mix_x86_32_body_scalar(key: &[u8], hash: &mut u32) {
    debug_assert!(key.len().is_multiple_of(4));
    let mut offset = 0;
    while offset < key.len() {
        let block = read_u32_le(key, offset)
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
pub(super) fn finish_x86_32(key: &[u8], hash: u32, offset: usize) -> u32 {
    finish_x86_32_tail(&key[offset..], hash, key.len() as u32)
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
