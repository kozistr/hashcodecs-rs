# hashcodecs

[![CI](https://img.shields.io/github/actions/workflow/status/kozistr/hashcodecs-rs/ci.yml?branch=main&style=for-the-badge&logo=github)](https://github.com/kozistr/hashcodecs-rs/actions/workflows/ci.yml)
[![Codecov](https://codecov.io/gh/kozistr/hashcodecs-rs/graph/badge.svg)](https://app.codecov.io/gh/kozistr/hashcodecs-rs)
[![PyPI](https://img.shields.io/pypi/v/hashcodecs?style=for-the-badge&logo=pypi)](https://pypi.org/project/hashcodecs/)
[![Python](https://img.shields.io/pypi/pyversions/hashcodecs?style=for-the-badge&logo=python)](https://pypi.org/project/hashcodecs/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-brightgreen?style=for-the-badge)](https://github.com/kozistr/hashcodecs-rs#license)
[![Downloads](https://img.shields.io/pypi/dm/hashcodecs?style=for-the-badge&label=downloads)](https://pypi.org/project/hashcodecs/)

`hashcodecs` provides runtime-dispatched SIMD Base64 codecs and fast, reference-compatible MurmurHash3 functions for Rust and Python.

## Design

- Runtime-dispatched SIMD Base64 with portable scalar fallbacks.
- Reference-compatible MurmurHash3 with SIMD acceleration.
- Rust and Python `*_into` APIs for caller-managed output buffers.
- A familiar Python Base64 API, including native batch encode and decode operations.

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
from hashcodecs import murmur3_32, murmur3_x64_128

assert base64.b64encode(b'hello') == b'aGVsbG8='
assert base64.b64decode(b'aGVsbG8=') == b'hello'
assert base64.b64encode_batch([b'hello', b'world']) == [b'aGVsbG8=', b'd29ybGQ=']
assert base64.b64decode_batch([b'aGVsbG8=', 'd29ybGQ=']) == [b'hello', b'world']
assert base64.b64encode(b'hello', padded=False) == b'aGVsbG8'
assert base64.b64decode(b'aGVsbG8', padded=False, canonical=True) == b'hello'
assert murmur3_32(b'hello') == 0x248BFA47

payload = b'hello'
encoded = bytearray(4 * ((len(payload) + 2) // 3))
encoded_len = base64.b64encode_into(payload, encoded)
decoded = bytearray(len(encoded))
decoded_len = base64.b64decode_into(encoded, decoded, validate=True)
assert encoded[:encoded_len] == b'aGVsbG8='
assert decoded[:decoded_len] == payload

hasher = murmur3_x64_128(seed=42)
hasher.update(b'hello')
assert hasher.hexdigest() == hasher.digest().hex()
```

Build an installable wheel and source distribution with:

```sh
uv build
```

# Benchmark

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, 50 Rust samples, and 15 Python samples. Returned-output allocation is included except in the reusable-buffer table. Higher is better.

## Run Locally

Comparison crates are development-only dependencies and are not included in consumer builds.

```sh
cargo bench --bench base64
cargo bench --bench murmur3
uv sync --group benchmark --no-install-project
uv run --no-project --with . python benchmarks/python_base64.py
uv run --no-project --with . python benchmarks/python_base64.py --into
uv run --no-project --with . python benchmarks/python_base64.py --bytearray-input
uv run --no-project --with . python benchmarks/python_base64_batch.py
uv run --no-project --with . python benchmarks/python_base64_batch.py --large
uv run --no-project --with . python benchmarks/python_murmur3.py
uv run --no-project --with . python benchmarks/python_murmur3.py --incremental
```

## Base64: Rust

| Alphabet | Input | Operation | hashcodecs | `base64` | `base64-turbo` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | **20.41 GiB/s** | 5.81 GiB/s | 18.29 GiB/s |
|  | 4 KiB | decode | **27.11 GiB/s** | 4.33 GiB/s | 16.95 GiB/s |
|  | 1 MiB | encode | **42.38 GiB/s** | 5.16 GiB/s | 19.60 GiB/s |
|  | 1 MiB | decode | **31.07 GiB/s** | 4.10 GiB/s | 18.17 GiB/s |
|  | 32 MiB | encode | **11.97 GiB/s** | 3.28 GiB/s | 10.94 GiB/s |
|  | 32 MiB | decode | **11.51 GiB/s** | 3.35 GiB/s | 10.45 GiB/s |
| URL-safe | 4 KiB | encode | **20.46 GiB/s** | 5.81 GiB/s | 18.31 GiB/s |
|  | 4 KiB | decode | **25.65 GiB/s** | 4.35 GiB/s | 16.96 GiB/s |
|  | 1 MiB | encode | **42.56 GiB/s** | 5.18 GiB/s | 19.66 GiB/s |
|  | 1 MiB | decode | **29.42 GiB/s** | 4.08 GiB/s | 18.16 GiB/s |
|  | 32 MiB | encode | **11.86 GiB/s** | 3.29 GiB/s | 11.09 GiB/s |
|  | 32 MiB | decode | **11.67 GiB/s** | 3.36 GiB/s | 10.62 GiB/s |

## MurmurHash3: Rust

| Variant | Input | hashcodecs | `murmur3` | `murmurs` | `fastmurmur3` | `mm3h` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| x86 32-bit | 4 KiB | **4.30 GiB/s** | 2.60 GiB/s | 4.09 GiB/s | n/a | **4.30 GiB/s** |
|  | 1 MiB | **4.26 GiB/s** | 2.59 GiB/s | 4.04 GiB/s | n/a | 4.25 GiB/s |
|  | 32 MiB | **4.21 GiB/s** | 2.54 GiB/s | 3.87 GiB/s | n/a | 4.03 GiB/s |
| x86 128-bit | 4 KiB | **9.54 GiB/s** | 4.97 GiB/s | 8.64 GiB/s | n/a | n/a |
|  | 1 MiB | **9.84 GiB/s** | 5.06 GiB/s | 8.77 GiB/s | n/a | n/a |
|  | 32 MiB | **9.63 GiB/s** | 4.84 GiB/s | 6.05 GiB/s | n/a | n/a |
| x64 128-bit | 4 KiB | **10.76 GiB/s** | 6.83 GiB/s | 9.45 GiB/s | 10.06 GiB/s | 9.54 GiB/s |
|  | 1 MiB | **10.84 GiB/s** | 6.89 GiB/s | 9.51 GiB/s | 10.04 GiB/s | 9.51 GiB/s |
|  | 32 MiB | **10.12 GiB/s** | 6.11 GiB/s | 6.64 GiB/s | 7.25 GiB/s | 6.70 GiB/s |

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

| Alphabet | Input | Operation | hashcodecs | CPython `base64` | `pybase64` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | 12.71 GiB/s | 0.46 GiB/s | **13.27 GiB/s** |
|  | 4 KiB | decode | **15.76 GiB/s** | 1.07 GiB/s | 8.07 GiB/s |
|  | 1 MiB | encode | **3.72 GiB/s** | 0.43 GiB/s | 3.60 GiB/s |
|  | 1 MiB | decode | **4.60 GiB/s** | 0.94 GiB/s | 4.47 GiB/s |
|  | 32 MiB | encode | **3.10 GiB/s** | 0.45 GiB/s | 2.83 GiB/s |
|  | 32 MiB | decode | 3.58 GiB/s | 0.96 GiB/s | **3.64 GiB/s** |
| URL-safe | 4 KiB | encode | **12.13 GiB/s** | 0.39 GiB/s | 1.12 GiB/s |
|  | 4 KiB | decode | **10.89 GiB/s** | 0.70 GiB/s | 1.48 GiB/s |
|  | 1 MiB | encode | **3.43 GiB/s** | 0.36 GiB/s | 0.92 GiB/s |
|  | 1 MiB | decode | **4.14 GiB/s** | 0.63 GiB/s | 1.49 GiB/s |
|  | 32 MiB | encode | **2.93 GiB/s** | 0.36 GiB/s | 0.91 GiB/s |
|  | 32 MiB | decode | **3.57 GiB/s** | 0.60 GiB/s | 1.50 GiB/s |

## MurmurHash3: Python

| Variant | API | Input | hashcodecs | `mmh3` |
| --- | --- | --- | ---: | ---: |
| x86 32-bit | one-shot | 4 KiB | **3.79 GiB/s** | 3.71 GiB/s |
|  |  | 1 MiB | **3.98 GiB/s** | 3.83 GiB/s |
|  |  | 32 MiB | **3.97 GiB/s** | 3.66 GiB/s |
|  | incremental | 4 KiB | **3.58 GiB/s** | **3.58 GiB/s** |
|  |  | 1 MiB | **3.96 GiB/s** | 3.81 GiB/s |
|  |  | 32 MiB | **3.92 GiB/s** | 3.74 GiB/s |
| x86 128-bit | one-shot | 4 KiB | 8.09 GiB/s | **8.21 GiB/s** |
|  |  | 1 MiB | **9.22 GiB/s** | 8.87 GiB/s |
|  |  | 32 MiB | **9.12 GiB/s** | 6.15 GiB/s |
|  | incremental | 4 KiB | **7.02 GiB/s** | 0.77 GiB/s |
|  |  | 1 MiB | **9.17 GiB/s** | 0.80 GiB/s |
|  |  | 32 MiB | **9.01 GiB/s** | 0.79 GiB/s |
| x64 128-bit | one-shot | 4 KiB | 8.68 GiB/s | **9.46 GiB/s** |
|  |  | 1 MiB | 10.10 GiB/s | **10.24 GiB/s** |
|  |  | 32 MiB | **9.54 GiB/s** | 6.61 GiB/s |
|  | incremental | 4 KiB | 7.70 GiB/s | **7.99 GiB/s** |
|  |  | 1 MiB | **10.01 GiB/s** | 9.28 GiB/s |
|  |  | 32 MiB | **9.41 GiB/s** | 7.34 GiB/s |

Reusable-buffer and mutable-input results are available in [BENCHMARK.md](https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md).

## SIMD References

The SIMD implementation follows the approach described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605), with AVX-512 VBMI and AArch64 NEON backends selected automatically when available.
