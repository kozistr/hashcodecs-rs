//! MurmurHash3 reference-compatible functions with runtime SIMD dispatch.
//!
//! SIMD kernels premix independent input words in parallel. The canonical
//! loop-carried state transitions remain ordered exactly as specified.

#[inline(always)]
fn fmix32(mut hash: u32) -> u32 {
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85eb_ca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2_ae35);
    hash ^ (hash >> 16)
}

#[inline(always)]
fn fmix64(mut hash: u64) -> u64 {
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    hash ^ (hash >> 33)
}

#[inline(always)]
fn read_u16_le(input: &[u8], offset: usize) -> u16 {
    debug_assert!(offset + 2 <= input.len());
    unsafe { u16::from_le((input.as_ptr().add(offset).cast::<u16>()).read_unaligned()) }
}

#[inline(always)]
fn read_u32_le(input: &[u8], offset: usize) -> u32 {
    debug_assert!(offset + 4 <= input.len());
    // Bounds are checked by the caller. Unaligned reads avoid a temporary
    // array and compile to one load on the x86 CPUs this crate targets.
    unsafe { u32::from_le((input.as_ptr().add(offset).cast::<u32>()).read_unaligned()) }
}

#[inline(always)]
fn read_u64_le(input: &[u8], offset: usize) -> u64 {
    debug_assert!(offset + 8 <= input.len());
    // See `read_u32_le`; this is portable because `from_le` normalizes endian.
    unsafe { u64::from_le((input.as_ptr().add(offset).cast::<u64>()).read_unaligned()) }
}

#[inline(always)]
fn read_partial_u64_le(input: &[u8]) -> u64 {
    debug_assert!(input.len() <= 8);
    match input.len() {
        0 => 0,
        1 => input[0] as u64,
        2 => read_u16_le(input, 0) as u64,
        3 => read_u16_le(input, 0) as u64 | ((input[2] as u64) << 16),
        4 => read_u32_le(input, 0) as u64,
        5 => read_u32_le(input, 0) as u64 | ((input[4] as u64) << 32),
        6 => read_u32_le(input, 0) as u64 | ((read_u16_le(input, 4) as u64) << 32),
        7 => {
            read_u32_le(input, 0) as u64
                | ((read_u16_le(input, 4) as u64) << 32)
                | ((input[6] as u64) << 48)
        }
        _ => {
            debug_assert_eq!(input.len(), 8);
            read_u64_le(input, 0)
        }
    }
}

#[inline]
fn consume_blocks<const BLOCK_SIZE: usize>(
    tail: &mut [u8; BLOCK_SIZE],
    tail_len: &mut usize,
    mut input: &[u8],
    mut consume: impl FnMut(&[u8]),
) {
    debug_assert!(BLOCK_SIZE.is_power_of_two());
    if *tail_len != 0 {
        let needed = BLOCK_SIZE - *tail_len;
        let copied = needed.min(input.len());
        tail[*tail_len..*tail_len + copied].copy_from_slice(&input[..copied]);
        *tail_len += copied;
        input = &input[copied..];
        if *tail_len != BLOCK_SIZE {
            return;
        }
        consume(tail);
        *tail_len = 0;
    }

    let body_len = input.len() & !(BLOCK_SIZE - 1);
    if body_len != 0 {
        consume(&input[..body_len]);
    }
    let remaining = &input[body_len..];
    tail[..remaining.len()].copy_from_slice(remaining);
    *tail_len = remaining.len();
}

/// Incremental state for the canonical MurmurHash3 x86 32-bit algorithm.
///
/// Data can be supplied in any chunk sizes. Cloning the value creates an
/// independent checkpoint, and digest does not consume or reset the state.
/// MurmurHash3 is non-cryptographic and must not be used where collision or
/// preimage resistance is required.
///
/// # Examples
///
///     use hashcodecs::Murmur3X86Hasher32;
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
    tail: [u8; 4],
    tail_len: usize,
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
    ///     use hashcodecs::{Murmur3X86Hasher32, murmur3_x86_32};
    ///
    ///     let hasher = Murmur3X86Hasher32::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x86_32(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hash: seed,
            tail: [0; 4],
            tail_len: 0,
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
    ///     use hashcodecs::{Murmur3X86Hasher32, murmur3_x86_32};
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
        consume_blocks(&mut self.tail, &mut self.tail_len, input, |blocks| {
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
    ///     use hashcodecs::Murmur3X86Hasher32;
    ///
    ///     let mut hasher = Murmur3X86Hasher32::default();
    ///     hasher.update(b"hello");
    ///     assert_eq!(hasher.digest(), 0x248b_fa47);
    ///     assert_eq!(hasher.digest(), 0x248b_fa47);
    ///
    #[inline]
    pub fn digest(&self) -> u32 {
        finish_x86_32_tail(&self.tail[..self.tail_len], self.hash, self.length)
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
///     use hashcodecs::murmur3_x86_32;
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
fn mix_x86_32_body(key: &[u8], hash: &mut u32) {
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
fn murmur3_x86_32_scalar(key: &[u8], seed: u32) -> u32 {
    let mut hash = seed;
    let block_end = key.len() & !3;
    mix_x86_32_body_scalar(&key[..block_end], &mut hash);
    finish_x86_32(key, hash, block_end)
}

#[inline]
fn mix_x86_32_body_scalar(key: &[u8], hash: &mut u32) {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;

    debug_assert!(key.len().is_multiple_of(4));
    let mut offset = 0;
    while offset < key.len() {
        let block = read_u32_le(key, offset)
            .wrapping_mul(C1)
            .rotate_left(15)
            .wrapping_mul(C2);
        *hash = mix_x86_32_hash(*hash, block);
        offset += 4;
    }
}

#[inline(always)]
fn mix_x86_32_hash(mut hash: u32, block: u32) -> u32 {
    hash ^= block;
    hash.rotate_left(13)
        .wrapping_mul(5)
        .wrapping_add(0xe654_6b64)
}

#[inline(always)]
fn finish_x86_32(key: &[u8], hash: u32, offset: usize) -> u32 {
    finish_x86_32_tail(&key[offset..], hash, key.len() as u32)
}

#[inline(always)]
fn finish_x86_32_tail(tail_bytes: &[u8], mut hash: u32, length: u32) -> u32 {
    const C1: u32 = 0xcc9e_2d51;
    const C2: u32 = 0x1b87_3593;
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
        hash ^= tail.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
    }

    fmix32(hash ^ length)
}

/// Incremental state for the canonical MurmurHash3 x86 128-bit algorithm.
///
/// Input may be split at arbitrary byte boundaries. The four digest words are
/// ordered exactly like the original reference implementation. MurmurHash3 is
/// non-cryptographic.
///
/// # Examples
///
///     use hashcodecs::Murmur3X86Hasher128;
///
///     let mut hasher = Murmur3X86Hasher128::new(7);
///     hasher.update(b"hello");
///     let checkpoint = hasher.clone();
///     hasher.update(b" world");
///     assert_ne!(hasher.digest(), checkpoint.digest());
///
#[derive(Clone, Debug)]
pub struct Murmur3X86Hasher128 {
    hashes: [u32; 4],
    tail: [u8; 16],
    tail_len: usize,
    length: u32,
}

impl Murmur3X86Hasher128 {
    /// Creates an empty x86 128-bit hasher with the supplied seed.
    ///
    /// # Arguments
    ///
    /// * seed - The initial unsigned 32-bit seed applied to all four lanes.
    ///
    /// # Returns
    ///
    /// A hasher ready to receive bytes through update.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::{Murmur3X86Hasher128, murmur3_x86_128};
    ///
    ///     let hasher = Murmur3X86Hasher128::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x86_128(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hashes: [seed; 4],
            tail: [0; 16],
            tail_len: 0,
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
    ///     use hashcodecs::{Murmur3X86Hasher128, murmur3_x86_128};
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
        consume_blocks(&mut self.tail, &mut self.tail_len, input, |blocks| {
            mix_x86_128_body(blocks, hashes);
        });
    }

    /// Computes the current 128-bit digest without consuming the state.
    ///
    /// # Returns
    ///
    /// Four 32-bit words in canonical low-to-high reference order.
    ///
    /// # Examples
    ///
    ///     use hashcodecs::Murmur3X86Hasher128;
    ///
    ///     let mut hasher = Murmur3X86Hasher128::default();
    ///     hasher.update(b"hello");
    ///     let first = hasher.digest();
    ///     assert_eq!(first, hasher.digest());
    ///
    #[inline]
    pub fn digest(&self) -> [u32; 4] {
        finish_x86_128_tail(&self.tail[..self.tail_len], self.hashes, self.length)
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
/// * key - The bytes to hash.
/// * seed - The initial unsigned 32-bit seed applied to all four lanes.
///
/// # Returns
///
/// Four unsigned 32-bit words in canonical low-to-high reference order. To
/// serialize the digest used by the Python API, concatenate each word's
/// little-endian bytes.
///
/// # Examples
///
///     use hashcodecs::murmur3_x86_128;
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
pub fn murmur3_x86_128(key: &[u8], seed: u32) -> [u32; 4] {
    let mut hashes = [seed; 4];
    let block_end = key.len() & !15;
    mix_x86_128_body(&key[..block_end], &mut hashes);
    finish_x86_128(key, hashes, block_end)
}

#[inline]
fn finish_x86_128(key: &[u8], hashes: [u32; 4], offset: usize) -> [u32; 4] {
    finish_x86_128_tail(&key[offset..], hashes, key.len() as u32)
}

#[inline]
fn finish_x86_128_tail(tail: &[u8], mut hashes: [u32; 4], length: u32) -> [u32; 4] {
    debug_assert!(tail.len() < 16);
    let mut blocks = [0u32; 4];
    for (index, byte) in tail.iter().copied().enumerate() {
        blocks[index / 4] |= (byte as u32) << ((index % 4) * 8);
    }
    if tail.len() > 12 {
        hashes[3] ^= blocks[3]
            .wrapping_mul(0xa1e3_8b93)
            .rotate_left(18)
            .wrapping_mul(0x239b_961b);
    }
    if tail.len() > 8 {
        hashes[2] ^= blocks[2]
            .wrapping_mul(0x38b3_4ae5)
            .rotate_left(17)
            .wrapping_mul(0xa1e3_8b93);
    }
    if tail.len() > 4 {
        hashes[1] ^= blocks[1]
            .wrapping_mul(0xab0e_9789)
            .rotate_left(16)
            .wrapping_mul(0x38b3_4ae5);
    }
    if !tail.is_empty() {
        hashes[0] ^= blocks[0]
            .wrapping_mul(0x239b_961b)
            .rotate_left(15)
            .wrapping_mul(0xab0e_9789);
    }

    finalize_x86_128(hashes, length)
}

/// Incremental state for the canonical MurmurHash3 x64 128-bit algorithm.
///
/// Input may be split at arbitrary byte boundaries. The two digest words are
/// ordered exactly like the original reference implementation. MurmurHash3 is
/// non-cryptographic.
///
/// # Examples
///
///     use hashcodecs::Murmur3X64Hasher128;
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
    tail: [u8; 16],
    tail_len: usize,
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
    ///     use hashcodecs::{Murmur3X64Hasher128, murmur3_x64_128};
    ///
    ///     let hasher = Murmur3X64Hasher128::new(42);
    ///     assert_eq!(hasher.digest(), murmur3_x64_128(b"", 42));
    ///
    #[inline]
    pub const fn new(seed: u32) -> Self {
        Self {
            hashes: [seed as u64; 2],
            tail: [0; 16],
            tail_len: 0,
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
    ///     use hashcodecs::{Murmur3X64Hasher128, murmur3_x64_128};
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
        consume_blocks(&mut self.tail, &mut self.tail_len, input, |blocks| {
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
    ///     use hashcodecs::Murmur3X64Hasher128;
    ///
    ///     let mut hasher = Murmur3X64Hasher128::default();
    ///     hasher.update(b"hello");
    ///     let first = hasher.digest();
    ///     assert_eq!(first, hasher.digest());
    ///
    #[inline]
    pub fn digest(&self) -> [u64; 2] {
        finish_x64_128_tail(&self.tail[..self.tail_len], self.hashes, self.length)
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
///     use hashcodecs::murmur3_x64_128;
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
fn murmur3_x64_128_inner(key: &[u8], seed: u64) -> [u64; 2] {
    let block_end = key.len() & !15;
    let mut hashes = [seed; 2];
    mix_x64_128_body(&key[..block_end], &mut hashes);
    finish_x64_128(key, hashes, block_end)
}

#[inline]
fn mix_x64_128_body(key: &[u8], hashes: &mut [u64; 2]) {
    debug_assert!(key.len().is_multiple_of(16));
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::x64_128(key.len(), capabilities);
        if unsafe { x86::try_mix_x64_128_body(key, hashes, selected, capabilities.has_bmi2()) } {
            return;
        }
    }
    mix_x64_128_body_scalar(key, hashes);
}

#[inline(never)]
#[cfg(test)]
fn murmur3_x64_128_scalar_inner(key: &[u8], seed: u64) -> [u64; 2] {
    let block_end = key.len() & !15;
    let mut hashes = [seed; 2];
    mix_x64_128_body_scalar(&key[..block_end], &mut hashes);
    finish_x64_128(key, hashes, block_end)
}

#[inline]
fn mix_x64_128_body_scalar(key: &[u8], hashes: &mut [u64; 2]) {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;

    debug_assert!(key.len().is_multiple_of(16));
    let mut hash1 = hashes[0];
    let mut hash2 = hashes[1];
    let mut input = key.as_ptr();
    let end = unsafe { input.add(key.len()) };
    while input < end {
        let value1 = u64::from_le(unsafe { input.cast::<u64>().read_unaligned() });
        let value2 = u64::from_le(unsafe { input.add(8).cast::<u64>().read_unaligned() });
        let block1 = value1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
        let block2 = value2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
        hash1 ^= block1;
        hash1 = hash1
            .rotate_left(27)
            .wrapping_add(hash2)
            .wrapping_mul(5)
            .wrapping_add(0x52dc_e729);
        hash2 ^= block2;
        hash2 = hash2
            .rotate_left(31)
            .wrapping_add(hash1)
            .wrapping_mul(5)
            .wrapping_add(0x3849_5ab5);
        input = unsafe { input.add(16) };
    }
    *hashes = [hash1, hash2];
}

#[inline]
fn finish_x64_128(key: &[u8], hashes: [u64; 2], offset: usize) -> [u64; 2] {
    finish_x64_128_tail(&key[offset..], hashes, key.len() as u64)
}

#[inline]
fn finish_x64_128_tail(tail: &[u8], mut hashes: [u64; 2], length: u64) -> [u64; 2] {
    const C1: u64 = 0x87c3_7b91_1142_53d5;
    const C2: u64 = 0x4cf5_ad43_2745_937f;
    debug_assert!(tail.len() < 16);
    if tail.len() > 8 {
        let block2 = read_partial_u64_le(&tail[8..]);
        hashes[1] ^= block2.wrapping_mul(C2).rotate_left(33).wrapping_mul(C1);
    }
    if !tail.is_empty() {
        let block1 = read_partial_u64_le(&tail[..tail.len().min(8)]);
        hashes[0] ^= block1.wrapping_mul(C1).rotate_left(31).wrapping_mul(C2);
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

#[inline]
fn mix_x86_128_body(key: &[u8], hashes: &mut [u32; 4]) {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        let capabilities = crate::backend::capabilities();
        let selected = dispatch::x86_128(key.len(), capabilities);
        if unsafe { x86::try_mix_x86_128_body(key, hashes, selected) } {
            return;
        }
    }
    mix_x86_128_body_scalar(key, hashes);
}

#[inline]
fn mix_x86_128_body_scalar(key: &[u8], hashes: &mut [u32; 4]) {
    const C1: [u32; 4] = [0x239b_961b, 0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93];
    const C2: [u32; 4] = [0xab0e_9789, 0x38b3_4ae5, 0xa1e3_8b93, 0x239b_961b];
    const ROTATE_K: [u32; 4] = [15, 16, 17, 18];

    let mut offset = 0;
    while offset < key.len() {
        let block1 = read_u32_le(key, offset)
            .wrapping_mul(C1[0])
            .rotate_left(ROTATE_K[0])
            .wrapping_mul(C2[0]);
        let block2 = read_u32_le(key, offset + 4)
            .wrapping_mul(C1[1])
            .rotate_left(ROTATE_K[1])
            .wrapping_mul(C2[1]);
        let block3 = read_u32_le(key, offset + 8)
            .wrapping_mul(C1[2])
            .rotate_left(ROTATE_K[2])
            .wrapping_mul(C2[2]);
        let block4 = read_u32_le(key, offset + 12)
            .wrapping_mul(C1[3])
            .rotate_left(ROTATE_K[3])
            .wrapping_mul(C2[3]);
        mix_x86_128_hashes(hashes, block1, block2, block3, block4);
        offset += 16;
    }
}

#[inline(always)]
fn mix_x86_128_hashes(hashes: &mut [u32; 4], block1: u32, block2: u32, block3: u32, block4: u32) {
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
fn finalize_x86_128(mut hashes: [u32; 4], length: u32) -> [u32; 4] {
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

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
mod dispatch;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn little_endian_loads_stay_within_the_slice() {
        let bytes: [u8; 16] = kani::any();
        let offset16: usize = kani::any();
        let offset32: usize = kani::any();
        let offset64: usize = kani::any();
        kani::assume(offset16 <= bytes.len() - 2);
        kani::assume(offset32 <= bytes.len() - 4);
        kani::assume(offset64 <= bytes.len() - 8);

        assert_eq!(
            read_u16_le(&bytes, offset16),
            u16::from_le_bytes([bytes[offset16], bytes[offset16 + 1]])
        );
        assert_eq!(
            read_u32_le(&bytes, offset32),
            u32::from_le_bytes([
                bytes[offset32],
                bytes[offset32 + 1],
                bytes[offset32 + 2],
                bytes[offset32 + 3],
            ])
        );
        assert_eq!(
            read_u64_le(&bytes, offset64),
            u64::from_le_bytes([
                bytes[offset64],
                bytes[offset64 + 1],
                bytes[offset64 + 2],
                bytes[offset64 + 3],
                bytes[offset64 + 4],
                bytes[offset64 + 5],
                bytes[offset64 + 6],
                bytes[offset64 + 7],
            ])
        );
    }

    #[kani::proof]
    #[kani::unwind(9)]
    fn scalar_block_loops_and_partial_loads_stay_in_bounds() {
        let input: [u8; 32] = kani::any();
        let partial_length: usize = kani::any();
        kani::assume(partial_length <= 8);
        let _ = read_partial_u64_le(&input[..partial_length]);

        let mut hash32: u32 = kani::any();
        mix_x86_32_body_scalar(&input, &mut hash32);
        let mut hashes_x86: [u32; 4] = kani::any();
        mix_x86_128_body_scalar(&input, &mut hashes_x86);
        let mut hashes_x64: [u64; 2] = kani::any();
        mix_x64_128_body_scalar(&input, &mut hashes_x64);
    }
}

#[cfg(all(test, miri))]
mod miri_tests {
    use super::*;

    #[test]
    fn one_shot_and_incremental_boundaries_are_defined() {
        const LENGTHS: &[usize] = &[
            0, 1, 2, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129, 255, 256, 257,
            511, 512, 513, 1025,
        ];
        for &length in LENGTHS {
            let input = (0..length)
                .map(|index| (index as u8).wrapping_mul(29).wrapping_add(13))
                .collect::<Vec<_>>();
            for &seed in &[0, 1, u32::MAX] {
                let expected32 = murmur3_x86_32(&input, seed);
                let expected_x86 = murmur3_x86_128(&input, seed);
                let expected_x64 = murmur3_x64_128(&input, seed);
                for &chunk_size in &[1, 3, 7, 16, 31, 64] {
                    let mut hash32 = Murmur3X86Hasher32::new(seed);
                    let mut hash_x86 = Murmur3X86Hasher128::new(seed);
                    let mut hash_x64 = Murmur3X64Hasher128::new(seed);
                    for chunk in input.chunks(chunk_size) {
                        hash32.update(chunk);
                        hash_x86.update(chunk);
                        hash_x64.update(chunk);
                    }
                    assert_eq!(hash32.digest(), expected32);
                    assert_eq!(hash_x86.digest(), expected_x86);
                    assert_eq!(hash_x64.digest(), expected_x64);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    use crate::backend::{self as cpu, SimdBackend};
    use std::io::Cursor;

    fn x86_words_as_u128(words: [u32; 4]) -> u128 {
        let mut bytes = [0; 16];
        for (index, word) in words.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        u128::from_le_bytes(bytes)
    }

    fn x64_words_as_u128(words: [u64; 2]) -> u128 {
        (words[0] as u128) | ((words[1] as u128) << 64)
    }

    #[test]
    fn dispatch_thresholds_are_explicit_and_feature_gated() {
        use crate::backend::{Capabilities, SimdBackend as Simd};
        use dispatch::Backend::{Avx2, Scalar, Sse41};
        let caps = |backend| Capabilities::for_backends(&[backend]);

        assert_eq!(dispatch::x86_32(15, caps(Simd::Avx2)), Scalar);
        assert_eq!(dispatch::x86_32(31, caps(Simd::Avx2)), Scalar);
        assert_eq!(dispatch::x86_32(16, caps(Simd::Sse41)), Sse41);
        assert_eq!(dispatch::x86_32(32, caps(Simd::Avx2)), Avx2);
        assert_eq!(dispatch::x86_32(usize::MAX, caps(Simd::Scalar)), Scalar);

        assert_eq!(dispatch::x86_128(255, caps(Simd::Avx2)), Scalar);
        assert_eq!(dispatch::x86_128(256, caps(Simd::Avx2)), Avx2);
        assert_eq!(
            dispatch::x86_128(16 * 1024 * 1024 - 1, caps(Simd::Sse41)),
            Scalar
        );
        assert_eq!(
            dispatch::x86_128(16 * 1024 * 1024, caps(Simd::Sse41)),
            Sse41
        );
        assert_eq!(dispatch::x86_128(usize::MAX, caps(Simd::Scalar)), Scalar);

        assert_eq!(dispatch::x64_128(15, caps(Simd::Avx2)), Scalar);
        assert_eq!(dispatch::x64_128(16, caps(Simd::Sse41)), Sse41);
        assert_eq!(dispatch::x64_128(32, caps(Simd::Avx2)), Avx2);
        assert_eq!(dispatch::x64_128(8 * 1024 * 1024, caps(Simd::Sse41)), Sse41);
        assert_eq!(
            dispatch::x64_128(8 * 1024 * 1024 + 1, caps(Simd::Sse41)),
            Scalar
        );
    }

    #[test]
    fn incremental_hashers_match_one_shot_for_all_tail_lengths() {
        let seeds = [0, 1, u32::MAX];
        let chunk_sizes = [1, 2, 3, 4, 7, 16, 31, 64];
        for length in 0..=128 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(29).wrapping_add(7))
                .collect();
            for seed in seeds {
                for chunk_size in chunk_sizes {
                    let mut x86_32 = Murmur3X86Hasher32::new(seed);
                    let mut x86_128 = Murmur3X86Hasher128::new(seed);
                    let mut x64_128 = Murmur3X64Hasher128::new(seed);
                    for chunk in input.chunks(chunk_size) {
                        x86_32.update(chunk);
                        x86_128.update(chunk);
                        x64_128.update(chunk);
                    }
                    assert_eq!(x86_32.digest(), murmur3_x86_32(&input, seed));
                    assert_eq!(x86_128.digest(), murmur3_x86_128(&input, seed));
                    assert_eq!(x64_128.digest(), murmur3_x64_128(&input, seed));
                }
            }
        }

        let mut original = Murmur3X64Hasher128::default();
        original.update(b"prefix");
        let snapshot = original.clone();
        original.update(b"-suffix");
        assert_eq!(snapshot.digest(), murmur3_x64_128(b"prefix", 0));
        assert_eq!(original.digest(), murmur3_x64_128(b"prefix-suffix", 0));
        assert_eq!(
            Murmur3X86Hasher32::default().digest(),
            murmur3_x86_32(b"", 0)
        );
        assert_eq!(
            Murmur3X86Hasher128::default().digest(),
            murmur3_x86_128(b"", 0)
        );
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn assert_x86_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
        let block_end = input.len() & !15;
        let capabilities = cpu::capabilities();
        let supported = [
            capabilities
                .supports(SimdBackend::Sse41)
                .then_some(dispatch::Backend::Sse41),
            capabilities
                .supports(SimdBackend::Avx2)
                .then_some(dispatch::Backend::Avx2),
        ];
        for selected in supported.into_iter().flatten() {
            let mut hashes = [seed; 4];
            assert!(unsafe {
                x86::try_mix_x86_128_body(&input[..block_end], &mut hashes, selected)
            });
            assert_eq!(
                x86_words_as_u128(finish_x86_128(input, hashes, block_end)),
                expected
            );
        }

        let mut unchanged = [seed; 4];
        assert!(!unsafe {
            x86::try_mix_x86_128_body(
                &input[..block_end],
                &mut unchanged,
                dispatch::Backend::Scalar,
            )
        });
        assert_eq!(unchanged, [seed; 4]);
    }

    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    fn assert_x64_128_simd_backends(input: &[u8], seed: u32, expected: u128) {
        let block_end = input.len() & !15;
        let capabilities = cpu::capabilities();
        let supported = [
            capabilities
                .supports(SimdBackend::Sse41)
                .then_some((dispatch::Backend::Sse41, false)),
            capabilities
                .supports(SimdBackend::Avx2)
                .then_some((dispatch::Backend::Avx2, false)),
            (capabilities.supports(SimdBackend::Avx2) && capabilities.has_bmi2())
                .then_some((dispatch::Backend::Avx2, true)),
        ];
        for (selected, bmi2) in supported.into_iter().flatten() {
            let mut hashes = [seed as u64; 2];
            assert!(unsafe {
                x86::try_mix_x64_128_body(&input[..block_end], &mut hashes, selected, bmi2)
            });
            assert_eq!(
                x64_words_as_u128(finish_x64_128(input, hashes, block_end)),
                expected
            );
        }

        let mut unchanged = [seed as u64; 2];
        assert!(!unsafe {
            x86::try_mix_x64_128_body(
                &input[..block_end],
                &mut unchanged,
                dispatch::Backend::Scalar,
                false,
            )
        });
        assert_eq!(unchanged, [seed as u64; 2]);
    }

    #[test]
    fn known_answer_vectors() {
        assert_eq!(read_partial_u64_le(&[]), 0);
        assert_eq!(murmur3_x86_32(b"hello", 0), 0x248b_fa47);
        assert_eq!(murmur3_x86_32(&[1, 2, 3], 0), 2_161_234_436);
        assert_eq!(
            murmur3_x86_128(&[1, 2, 3], 0),
            [4_127_286_497, 3_037_938_227, 3_037_938_227, 3_037_938_227]
        );
        assert_eq!(
            murmur3_x64_128(&[1, 2, 3], 0),
            [1_901_714_139_111_438_249, 5_282_241_052_699_499_109]
        );
    }

    #[test]
    fn scalar_x86_128_body_matches_the_reference() {
        let data: Vec<u8> = (0..255).map(|value| value as u8).collect();
        let mut scalar = [7; 4];
        mix_x86_128_body(&data[..240], &mut scalar);
        let scalar_hash = finalize_x86_128(scalar, 240);
        assert_eq!(
            x86_words_as_u128(scalar_hash),
            murmur3::murmur3_x86_128(&mut Cursor::new(&data[..240]), 7).unwrap()
        );
    }

    #[test]
    fn matches_the_reference_implementation_for_every_tail_length() {
        let seeds = [0, 1, 0xfeed_beef, u32::MAX];
        for length in 0..=512 {
            let input: Vec<u8> = (0..length)
                .map(|index| (index as u8).wrapping_mul(73).wrapping_add(19))
                .collect();
            for seed in seeds {
                let expected_x86_32 = murmur3::murmur3_32(&mut Cursor::new(&input), seed).unwrap();
                assert_eq!(
                    murmur3_x86_32(&input, seed),
                    expected_x86_32,
                    "x86_32 length={length} seed={seed}"
                );
                assert_eq!(
                    murmur3_x86_32_scalar(&input, seed),
                    expected_x86_32,
                    "scalar x86_32 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                {
                    let capabilities = cpu::capabilities();
                    let block_end = input.len() & !3;
                    let supported = [
                        capabilities
                            .supports(SimdBackend::Sse41)
                            .then_some(dispatch::Backend::Sse41),
                        capabilities
                            .supports(SimdBackend::Avx2)
                            .then_some(dispatch::Backend::Avx2),
                    ];
                    for selected in supported.into_iter().flatten() {
                        let mut hash = seed;
                        assert!(unsafe {
                            x86::try_mix_x86_32_body(&input[..block_end], &mut hash, selected)
                        });
                        assert_eq!(
                            finish_x86_32(&input, hash, block_end),
                            expected_x86_32,
                            "{selected:?} x86_32 length={length} seed={seed}"
                        );
                    }

                    let mut unchanged = seed;
                    assert!(!unsafe {
                        x86::try_mix_x86_32_body(
                            &input[..block_end],
                            &mut unchanged,
                            dispatch::Backend::Scalar,
                        )
                    });
                    assert_eq!(unchanged, seed);
                }

                let expected_x86_128 =
                    murmur3::murmur3_x86_128(&mut Cursor::new(&input), seed).unwrap();
                let x86 = x86_words_as_u128(murmur3_x86_128(&input, seed));
                assert_eq!(x86, expected_x86_128, "x86_128 length={length} seed={seed}");
                let block_end = input.len() & !15;
                let mut scalar_x86_128 = [seed; 4];
                mix_x86_128_body_scalar(&input[..block_end], &mut scalar_x86_128);
                assert_eq!(
                    x86_words_as_u128(finish_x86_128(&input, scalar_x86_128, block_end)),
                    expected_x86_128,
                    "scalar x86_128 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                assert_x86_128_simd_backends(&input, seed, expected_x86_128);

                let x64 = x64_words_as_u128(murmur3_x64_128(&input, seed));
                let expected_x64_128 =
                    murmur3::murmur3_x64_128(&mut Cursor::new(&input), seed).unwrap();
                assert_eq!(x64, expected_x64_128, "x64_128 length={length} seed={seed}");
                assert_eq!(
                    x64_words_as_u128(murmur3_x64_128_scalar_inner(&input, seed as u64)),
                    expected_x64_128,
                    "scalar x64_128 length={length} seed={seed}"
                );
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                assert_x64_128_simd_backends(&input, seed, expected_x64_128);
            }
        }
    }
}
