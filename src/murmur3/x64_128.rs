use super::block_buffer::{BlockBuffer, FullBlocks};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
use super::dispatch;
use super::primitives::{fmix64, read_partial_u64_le};
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub(super) mod x86;

pub(super) const X64_128_C1: u64 = 0x87c3_7b91_1142_53d5;
pub(super) const X64_128_C2: u64 = 0x4cf5_ad43_2745_937f;

/// Incremental state for the canonical MurmurHash3 x64 128-bit algorithm.
///
/// Input may be split at arbitrary byte boundaries. The two digest words are
/// ordered exactly like the original reference implementation. MurmurHash3 is
/// non-cryptographic.
///
/// # Examples
///
///     use hashcodecs::murmur3::Murmur3X64Hasher128;
///
///     let mut hasher = Murmur3X64Hasher128::new(7);
///     hasher.update(b"hello");
///     let checkpoint = hasher.clone();
///     hasher.update(b" world");
///     assert_ne!(hasher.digest(), checkpoint.digest());
///
#[derive(Clone, Debug)]
pub struct Murmur3X64Hasher128 {
    hashes: [u64; 2],
    tail: BlockBuffer<16>,
    length: u64,
}

impl Murmur3X64Hasher128 {
    /// Creates an empty x64 128-bit hasher with the supplied seed.
    ///
    /// # Arguments
    ///
    /// * seed - The initial unsigned 32-bit seed applied to both lanes.
    ///
    /// # Returns
    ///
    /// A hasher ready to receive bytes through update.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::{Murmur3X64Hasher128, murmur3_x64_128};
    ///
    ///     let hasher = Murmur3X64Hasher128::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x64_128(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hashes: [seed as u64; 2],
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
    ///     use hashcodecs::murmur3::{Murmur3X64Hasher128, murmur3_x64_128};
    ///
    ///     let mut hasher = Murmur3X64Hasher128::new(7);
    ///     hasher.update(b"hel");
    ///     hasher.update(b"lo");
    ///     assert_eq!(hasher.digest(), murmur3_x64_128(b"hello", 7));
    ///
    #[inline]
    pub fn update(&mut self, input: &[u8]) {
        self.length = self.length.wrapping_add(input.len() as u64);
        let hashes = &mut self.hashes;
        self.tail.consume(input, |blocks| {
            mix_x64_128_body(blocks, hashes);
        });
    }

    /// Computes the current 128-bit digest without consuming the state.
    ///
    /// # Returns
    ///
    /// Two 64-bit words in canonical low-to-high reference order.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::murmur3::Murmur3X64Hasher128;
    ///
    ///     let mut hasher = Murmur3X64Hasher128::default();
    ///     hasher.update(b"hello");
    ///     let first = hasher.digest();
    ///     assert_eq!(first, hasher.digest());
    ///
    #[inline]
    pub fn digest(&self) -> [u64; 2] {
        finish_x64_128_tail(self.tail.remaining(), self.hashes, self.length)
    }
}

impl Default for Murmur3X64Hasher128 {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Computes the canonical MurmurHash3 x64 128-bit hash in one call.
///
/// # Arguments
///
/// * key - The bytes to hash.
/// * seed - The initial unsigned 32-bit seed applied to both lanes.
///
/// # Returns
///
/// Two unsigned 64-bit words in canonical low-to-high reference order. To
/// serialize the digest used by the Python API, concatenate each word's
/// little-endian bytes.
///
/// # Examples
///
///     use hashcodecs::murmur3::murmur3_x64_128;
///
///     let words = murmur3_x64_128(&[1, 2, 3], 0);
///     let bytes: Vec<_> = words.into_iter().flat_map(u64::to_le_bytes).collect();
///     assert_eq!(
///         bytes,
///         [
///             0xa9, 0x37, 0x13, 0x0e, 0xef, 0x3e, 0x64, 0x1a,
///             0x65, 0x9a, 0x23, 0x3c, 0x40, 0x4a, 0x4e, 0x49,
///         ],
///     );
///
#[inline(always)]
pub fn murmur3_x64_128(key: &[u8], seed: u32) -> [u64; 2] {
    murmur3_x64_128_inner(key, seed as u64)
}

#[inline(never)]
pub(super) fn murmur3_x64_128_inner(key: &[u8], seed: u64) -> [u64; 2] {
    let (blocks, tail) = FullBlocks::<16>::split(key);
    let mut hashes = [seed; 2];
    mix_x64_128_body(blocks, &mut hashes);
    finish_x64_128_tail(tail, hashes, key.len() as u64)
}

#[inline]
pub(super) fn mix_x64_128_body(blocks: FullBlocks<'_, 16>, hashes: &mut [u64; 2]) {
    if blocks.len() == 0 {
        return;
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::x64_128(blocks.len(), capabilities);
        mix_x64_128_body_with_backend(blocks, hashes, selected, capabilities.has_bmi2());
    }
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    mix_x64_128_body_scalar(blocks, hashes);
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[inline]
pub(super) fn mix_x64_128_body_with_backend(
    blocks: FullBlocks<'_, 16>,
    hashes: &mut [u64; 2],
    backend: dispatch::Backend,
    has_bmi2: bool,
) {
    if unsafe { x86::try_mix_x64_128_body(blocks, hashes, backend, has_bmi2) } {
        return;
    }
    mix_x64_128_body_scalar(blocks, hashes);
}

#[inline(never)]
#[cfg(test)]
pub(super) fn murmur3_x64_128_scalar_inner(key: &[u8], seed: u64) -> [u64; 2] {
    let (blocks, tail) = FullBlocks::<16>::split(key);
    let mut hashes = [seed; 2];
    mix_x64_128_body_scalar(blocks, &mut hashes);
    finish_x64_128_tail(tail, hashes, key.len() as u64)
}

#[inline]
pub(super) fn mix_x64_128_body_scalar(blocks: FullBlocks<'_, 16>, hashes: &mut [u64; 2]) {
    let key = blocks.as_bytes();
    let mut hash1 = hashes[0];
    let mut hash2 = hashes[1];
    let mut input = key.as_ptr();
    let end = unsafe { input.add(key.len()) };
    while input < end {
        let value1 = u64::from_le(unsafe { input.cast::<u64>().read_unaligned() });
        let value2 = u64::from_le(unsafe { input.add(8).cast::<u64>().read_unaligned() });
        let block1 = value1
            .wrapping_mul(X64_128_C1)
            .rotate_left(31)
            .wrapping_mul(X64_128_C2);
        let block2 = value2
            .wrapping_mul(X64_128_C2)
            .rotate_left(33)
            .wrapping_mul(X64_128_C1);
        mix_x64_128_hashes(&mut hash1, &mut hash2, block1, block2);
        input = unsafe { input.add(16) };
    }
    *hashes = [hash1, hash2];
}

#[inline]
#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
pub(super) fn finish_x64_128(key: &[u8], hashes: [u64; 2], offset: usize) -> [u64; 2] {
    finish_x64_128_tail(&key[offset..], hashes, key.len() as u64)
}

#[inline]
pub(super) fn finish_x64_128_tail(tail: &[u8], mut hashes: [u64; 2], length: u64) -> [u64; 2] {
    debug_assert!(tail.len() < 16);
    if tail.len() > 8 {
        let block2 = read_partial_u64_le(&tail[8..]);
        hashes[1] ^= block2
            .wrapping_mul(X64_128_C2)
            .rotate_left(33)
            .wrapping_mul(X64_128_C1);
    }
    if !tail.is_empty() {
        let block1 = read_partial_u64_le(&tail[..tail.len().min(8)]);
        hashes[0] ^= block1
            .wrapping_mul(X64_128_C1)
            .rotate_left(31)
            .wrapping_mul(X64_128_C2);
    }

    hashes[0] ^= length;
    hashes[1] ^= length;
    hashes[0] = hashes[0].wrapping_add(hashes[1]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes[0] = fmix64(hashes[0]);
    hashes[1] = fmix64(hashes[1]);
    hashes[0] = hashes[0].wrapping_add(hashes[1]);
    hashes[1] = hashes[1].wrapping_add(hashes[0]);
    hashes
}

#[inline(always)]
pub(super) fn mix_x64_128_hashes(hash1: &mut u64, hash2: &mut u64, block1: u64, block2: u64) {
    *hash1 ^= block1;
    *hash1 = hash1
        .rotate_left(27)
        .wrapping_add(*hash2)
        .wrapping_mul(5)
        .wrapping_add(0x52dc_e729);
    *hash2 ^= block2;
    *hash2 = hash2
        .rotate_left(31)
        .wrapping_add(*hash1)
        .wrapping_mul(5)
        .wrapping_add(0x3849_5ab5);
}
