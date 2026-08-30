//! This crate implements binary codecs and non-cryptographic hashes.
//!
//! The runtime selects an in-crate AVX-512, AVX2, SSE4.1, SSSE3, NEON, or scalar kernel.
//!
//! # Rust API
//!
//! ```
//! use hashcodecs::{base64, murmur3, xxhash};
//!
//! assert_eq!(base64::b64encode(b"hello"), "aGVsbG8=");
//! assert_eq!(murmur3::murmur3_x86_32(b"hello", 0), 0x248b_fa47);
//! assert_eq!(xxhash::xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
//! assert_eq!(
//!     xxhash::xxh3_128(b"", 0),
//!     [0x6001_c324_468d_497f, 0x99aa_06d3_0147_98d8],
//! );
//! ```

#![deny(missing_docs)]

// Python extension builds use mimalloc. Rust crate consumers keep their selected allocator.
// Kani and Miri cannot interpret native allocators or CPU intrinsics.
#[cfg(all(feature = "extension-module", not(any(kani, miri))))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod backend;
pub mod base64;
pub mod murmur3;
pub mod xxhash;

#[cfg(feature = "python")]
mod bindings;
