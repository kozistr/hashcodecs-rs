# hashcodecs

[![CI](https://img.shields.io/github/actions/workflow/status/kozistr/hashcodecs-rs/ci.yml?branch=main&style=for-the-badge&logo=github)](https://github.com/kozistr/hashcodecs-rs/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/hashcodecs?style=for-the-badge&logo=pypi)](https://pypi.org/project/hashcodecs/)
[![Python](https://img.shields.io/pypi/pyversions/hashcodecs?style=for-the-badge&logo=python)](https://pypi.org/project/hashcodecs/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-brightgreen?style=for-the-badge)](https://github.com/kozistr/hashcodecs-rs#license)
[![Downloads](https://img.shields.io/pypi/dm/hashcodecs?style=for-the-badge&label=downloads)](https://pypi.org/project/hashcodecs/)

`hashcodecs` provides runtime-dispatched SIMD Base64 codecs and fast, reference-compatible MurmurHash3 functions for Rust and Python.

## Design

- Base64 is implemented in this crate with runtime AVX-512 VBMI, AVX2, SSE4.1, SSSE3, and AArch64 NEON dispatch. Portable wheels select the best available SIMD backend and fall back to scalar code otherwise.
- Rust callers can reuse their own output buffers through the `*_into` APIs. Every backend writes exactly the documented prefix; it never depends on spare capacity after the destination slice.
- Rust-owned results use the `mimalloc` global allocator.
- Every MurmurHash3 variant uses runtime-dispatched AVX2 or SSE4.1 pre-mixing where its measured crossover beats the scalar loop. Ordered state transitions remain reference-compatible, and every variant has a portable scalar fallback.
- The Python Base64 surface follows the familiar `base64` names. Use `import hashcodecs.base64 as base64` when replacing standard Base64 calls.
- Large Python calls release the GIL. Strict decoding and valid default-mode decoding write directly into the returned `bytes`; malformed default-mode input uses a CPython-compatible lenient fallback.

## Install

```
pip3 install hashcodecs
```

## Usage

### Rust

```rust
let encoded = hashcodecs::b64encode(b"hello");
assert_eq!(encoded, "aGVsbG8=");

let mut output = [0_u8; 8];
let written = hashcodecs::b64encode_into(b"hello", &mut output).unwrap();
assert_eq!(&output[..written], b"aGVsbG8=");
assert_eq!(hashcodecs::murmur3_x86_32(b"hello", 0), 0x248b_fa47);
```

### Python

```python
import hashcodecs.base64 as base64
from hashcodecs import murmur3_32

assert base64.b64encode(b"hello") == b"aGVsbG8="
assert base64.b64decode(b"aGVsbG8=") == b"hello"
assert murmur3_32(b"hello") == 0x248BFA47
```

Build an installable wheel and source distribution with:

```sh
uv build
```

# Benchmark

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and output allocation included. Higher is better.

## Run Locally

Comparison crates are development-only dependencies and are not included in consumer builds.

```sh
cargo bench --bench base64
cargo bench --bench murmur3
uv run --no-project python benchmarks/python_base64.py
```

To time only this implementation:

```sh
cargo bench --bench base64 -- hashcodecs
cargo bench --bench murmur3 -- hashcodecs
uv run --no-project python benchmarks/python_base64.py --hashcodecs-only
```

## Base64: Rust

| Alphabet | Input | Operation | hashcodecs | `base64` | `base64-turbo` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | **18.17 GiB/s** | 5.40 GiB/s | 16.97 GiB/s |
|  | 4 KiB | decode | **16.26 GiB/s** | 4.05 GiB/s | 15.76 GiB/s |
|  | 1 MiB | encode | **19.29 GiB/s** | 4.79 GiB/s | 18.28 GiB/s |
|  | 1 MiB | decode | **18.41 GiB/s** | 3.79 GiB/s | 16.93 GiB/s |
|  | 32 MiB | encode | **10.29 GiB/s** | 3.08 GiB/s | 10.24 GiB/s |
|  | 32 MiB | decode | **10.12 GiB/s** | 3.12 GiB/s | 9.83 GiB/s |
| URL-safe | 4 KiB | encode | **18.19 GiB/s** | 5.39 GiB/s | 16.98 GiB/s |
|  | 4 KiB | decode | 14.28 GiB/s | 4.02 GiB/s | **15.77 GiB/s** |
|  | 1 MiB | encode | **19.27 GiB/s** | 4.82 GiB/s | 18.32 GiB/s |
|  | 1 MiB | decode | 16.28 GiB/s | 3.83 GiB/s | **16.99 GiB/s** |
|  | 32 MiB | encode | 10.35 GiB/s | 3.11 GiB/s | **10.45 GiB/s** |
|  | 32 MiB | decode | 9.83 GiB/s | 3.17 GiB/s | **9.85 GiB/s** |

## MurmurHash3: Rust

| Variant | Input | hashcodecs | `murmur3` | `murmurs` | `fastmurmur3` | `mm3h` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| x86 32-bit | 4 KiB | **4.01 GiB/s** | 2.43 GiB/s | 3.80 GiB/s | n/a | 4.01 GiB/s |
|  | 1 MiB | 3.98 GiB/s | 2.44 GiB/s | 3.80 GiB/s | n/a | **3.99 GiB/s** |
|  | 32 MiB | **3.98 GiB/s** | 2.42 GiB/s | 3.72 GiB/s | n/a | 3.84 GiB/s |
| x86 128-bit | 4 KiB | **8.91 GiB/s** | 4.67 GiB/s | 8.09 GiB/s | n/a | n/a |
|  | 1 MiB | **9.15 GiB/s** | 4.72 GiB/s | 8.22 GiB/s | n/a | n/a |
|  | 32 MiB | **9.10 GiB/s** | 4.56 GiB/s | 5.94 GiB/s | n/a | n/a |
| x64 128-bit | 4 KiB | 9.34 GiB/s | 6.37 GiB/s | 8.85 GiB/s | **9.38 GiB/s** | 8.96 GiB/s |
|  | 1 MiB | **9.40 GiB/s** | 6.45 GiB/s | 8.87 GiB/s | 9.39 GiB/s | 8.96 GiB/s |
|  | 32 MiB | **9.03 GiB/s** | 5.87 GiB/s | 6.42 GiB/s | 6.98 GiB/s | 6.45 GiB/s |

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

| Alphabet | Input | Operation | hashcodecs | CPython `base64` | `pybase64` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | 13.74 GiB/s | 0.49 GiB/s | **14.05 GiB/s** |
|  | 4 KiB | decode | **11.95 GiB/s** | 1.15 GiB/s | 8.47 GiB/s |
|  | 1 MiB | encode | **3.70 GiB/s** | 0.47 GiB/s | 3.40 GiB/s |
|  | 1 MiB | decode | **4.29 GiB/s** | 0.89 GiB/s | 4.24 GiB/s |
|  | 32 MiB | encode | **3.81 GiB/s** | 0.47 GiB/s | 3.61 GiB/s |
|  | 32 MiB | decode | **4.58 GiB/s** | 0.91 GiB/s | **4.58 GiB/s** |
| URL-safe | 4 KiB | encode | **13.54 GiB/s** | 0.41 GiB/s | 1.19 GiB/s |
|  | 4 KiB | decode | **8.89 GiB/s** | 0.73 GiB/s | 1.56 GiB/s |
|  | 1 MiB | encode | **3.70 GiB/s** | 0.36 GiB/s | 0.97 GiB/s |
|  | 1 MiB | decode | **4.07 GiB/s** | 0.57 GiB/s | 1.56 GiB/s |
|  | 32 MiB | encode | **3.88 GiB/s** | 0.37 GiB/s | 0.99 GiB/s |
|  | 32 MiB | decode | **4.30 GiB/s** | 0.58 GiB/s | 1.67 GiB/s |

## SIMD References

The AVX2 block structure uses the 24-byte encode and 32-byte decode arrangement described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605). The AVX-512 VBMI path processes 48-byte encode and 64-byte decode blocks with cross-lane byte permutations, while AArch64 uses 48-byte NEON encoding and 64-byte NEON decoding blocks. The Rust 1.89 MSRV is the first stable release that exposes the required AVX-512 intrinsics.
