# hashcodecs

[![CI](https://img.shields.io/github/actions/workflow/status/kozistr/hashcodecs-rs/ci.yml?branch=main&style=for-the-badge&logo=github)](https://github.com/kozistr/hashcodecs-rs/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/hashcodecs?style=for-the-badge&logo=pypi)](https://pypi.org/project/hashcodecs/)
[![Python](https://img.shields.io/pypi/pyversions/hashcodecs?style=for-the-badge&logo=python)](https://pypi.org/project/hashcodecs/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-brightgreen?style=for-the-badge)](https://github.com/kozistr/hashcodecs-rs#license)
[![Downloads](https://img.shields.io/pypi/dm/hashcodecs?style=for-the-badge&label=downloads)](https://pypi.org/project/hashcodecs/)

`hashcodecs` provides runtime-dispatched SIMD Base64 codecs and fast, reference-compatible MurmurHash3 functions for Rust and Python.

## Design

- Base64 is implemented in this crate with runtime AVX2 and SSSE3 dispatch. One portable wheel supports Intel and AMD CPUs, selecting SIMD where the OS and CPU allow it and a scalar fallback otherwise.
- Rust callers can reuse their own output buffers through the `*_into` APIs. Every backend writes exactly the documented prefix; it never depends on spare capacity after the destination slice.
- Rust-owned results use the `mimalloc` global allocator.
- MurmurHash3 uses direct little-endian block loads and tight scalar loops. Its state transitions are loop-carried, so the scalar implementation is faster than forcing wide-vector instructions for these hashes.
- The Python Base64 surface follows the familiar `base64` names. Use `import hashcodecs.base64 as base64` when replacing standard Base64 calls.
- Large Python calls release the GIL. Strict decoding and valid default-mode decoding write directly into the returned `bytes`; malformed default-mode input uses a CPython-compatible lenient fallback.

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

Run the Rust comparison benchmarks with:

```sh
cargo bench --bench base64
cargo bench --bench murmur3
```

Run the Python Base64 comparison with:

```sh
uv run --no-project python benchmarks/python_base64.py
```

Results are recorded below.

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
| Standard | 4 KiB | encode | **18.11 GiB/s** | 5.40 GiB/s | 17.01 GiB/s |
|  | 4 KiB | decode | **16.18 GiB/s** | 4.05 GiB/s | 15.83 GiB/s |
|  | 1 MiB | encode | **19.25 GiB/s** | 4.83 GiB/s | 18.28 GiB/s |
|  | 1 MiB | decode | **18.28 GiB/s** | 3.81 GiB/s | 16.78 GiB/s |
|  | 512 MiB | encode | **8.90 GiB/s** | 3.10 GiB/s | 8.88 GiB/s |
|  | 512 MiB | decode | **9.32 GiB/s** | 3.03 GiB/s | 9.12 GiB/s |
| URL-safe | 4 KiB | encode | **17.77 GiB/s** | 5.42 GiB/s | 17.02 GiB/s |
|  | 4 KiB | decode | 14.39 GiB/s | 4.04 GiB/s | **15.72 GiB/s** |
|  | 1 MiB | encode | **19.27 GiB/s** | 4.83 GiB/s | 18.30 GiB/s |
|  | 1 MiB | decode | 16.15 GiB/s | 3.81 GiB/s | **16.97 GiB/s** |
|  | 512 MiB | encode | **8.97 GiB/s** | 3.09 GiB/s | 8.92 GiB/s |
|  | 512 MiB | decode | **9.32 GiB/s** | 3.00 GiB/s | 9.20 GiB/s |

These measurements include output allocation and use the same public APIs available to callers.

## MurmurHash3: Rust

| Variant | Input | hashcodecs | `murmur3` | `murmurs` | `fastmurmur3` | `mm3h` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| x86 32-bit | 4 KiB | 3.75 GiB/s | 2.42 GiB/s | 3.81 GiB/s | n/a | **4.02 GiB/s** |
|  | 1 MiB | 3.81 GiB/s | 2.44 GiB/s | 3.79 GiB/s | n/a | **3.98 GiB/s** |
|  | 512 MiB | 3.58 GiB/s | 2.42 GiB/s | 3.58 GiB/s | n/a | **3.73 GiB/s** |
| x86 128-bit | 4 KiB | **8.18 GiB/s** | 4.66 GiB/s | 8.08 GiB/s | n/a | n/a |
|  | 1 MiB | **8.26 GiB/s** | 4.71 GiB/s | 8.19 GiB/s | n/a | n/a |
|  | 512 MiB | **5.61 GiB/s** | 4.44 GiB/s | 5.33 GiB/s | n/a | n/a |
| x64 128-bit | 4 KiB | 8.96 GiB/s | 6.38 GiB/s | 8.85 GiB/s | **9.40 GiB/s** | 8.87 GiB/s |
|  | 1 MiB | 8.95 GiB/s | 6.44 GiB/s | 8.87 GiB/s | **9.39 GiB/s** | 8.89 GiB/s |
|  | 512 MiB | 5.98 GiB/s | 5.56 GiB/s | 5.66 GiB/s | **6.56 GiB/s** | 5.73 GiB/s |

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

| Alphabet | Input | Operation | hashcodecs | CPython `base64` | `pybase64` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | 13.73 GiB/s | 0.49 GiB/s | **14.09 GiB/s** |
|  | 4 KiB | decode | **12.04 GiB/s** | 1.14 GiB/s | 8.50 GiB/s |
|  | 1 MiB | encode | **3.66 GiB/s** | 0.47 GiB/s | 3.32 GiB/s |
|  | 1 MiB | decode | 4.09 GiB/s | 0.89 GiB/s | **4.13 GiB/s** |
|  | 512 MiB | encode | 2.86 GiB/s | 0.44 GiB/s | **3.04 GiB/s** |
|  | 512 MiB | decode | 3.41 GiB/s | 0.94 GiB/s | **3.74 GiB/s** |
| URL-safe | 4 KiB | encode | **13.60 GiB/s** | 0.41 GiB/s | 1.19 GiB/s |
|  | 4 KiB | decode | **9.00 GiB/s** | 0.76 GiB/s | 1.56 GiB/s |
|  | 1 MiB | encode | **3.61 GiB/s** | 0.36 GiB/s | 0.96 GiB/s |
|  | 1 MiB | decode | **3.91 GiB/s** | 0.57 GiB/s | 1.55 GiB/s |
|  | 512 MiB | encode | **2.93 GiB/s** | 0.36 GiB/s | 0.91 GiB/s |
|  | 512 MiB | decode | **3.33 GiB/s** | 0.63 GiB/s | 1.53 GiB/s |

All Python values come from the same rebuilt wheel and pinned full-comparison run.

## SIMD References

The AVX2 block structure uses the 24-byte encode and 32-byte decode arrangement described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605). [Base64 Turbo](https://github.com/hacer-bark/base64-turbo) is included as a Rust comparator and is a useful direction for a future AVX-512 backend. This crate currently retains its Rust 1.85 MSRV, so its production runtime dispatch remains AVX2, SSSE3, and scalar.
