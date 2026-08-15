# hashcodecs

[![CI](https://img.shields.io/github/actions/workflow/status/kozistr/hashcodecs-rs/ci.yml?branch=main&style=for-the-badge&logo=github)](https://github.com/kozistr/hashcodecs-rs/actions/workflows/ci.yml)
[![Codecov](https://codecov.io/gh/kozistr/hashcodecs-rs/graph/badge.svg)](https://app.codecov.io/gh/kozistr/hashcodecs-rs)
[![PyPI](https://img.shields.io/pypi/v/hashcodecs?style=for-the-badge&logo=pypi)](https://pypi.org/project/hashcodecs/)
[![Python](https://img.shields.io/pypi/pyversions/hashcodecs?style=for-the-badge&logo=python)](https://pypi.org/project/hashcodecs/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-brightgreen?style=for-the-badge)](https://github.com/kozistr/hashcodecs-rs#license)
[![Downloads](https://img.shields.io/pypi/dm/hashcodecs?style=for-the-badge&label=downloads)](https://pypi.org/project/hashcodecs/)

`hashcodecs` provides runtime-dispatched SIMD Base64 codecs and fast, reference-compatible MurmurHash3 and XXH3 functions for Rust and Python.

## Design

- Runtime-dispatched SIMD Base64 with portable scalar fallbacks.
- Reference-compatible MurmurHash3 with SIMD acceleration.
- Bit-for-bit compatible XXH3-64 and XXH3-128 with runtime SIMD dispatch and native batch APIs.
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
assert_eq!(hashcodecs::xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
```

### Python

```python
import hashcodecs.base64 as base64
from hashcodecs import murmur3_32, murmur3_x64_128, xxh3_64, xxh3_128_batch

assert base64.b64encode(b'hello') == b'aGVsbG8='
assert base64.b64decode(b'aGVsbG8=') == b'hello'
assert base64.b64encode_batch([b'hello', b'world']) == [b'aGVsbG8=', b'd29ybGQ=']
assert base64.b64decode_batch([b'aGVsbG8=', 'd29ybGQ=']) == [b'hello', b'world']
assert base64.b64encode(b'hello', padded=False) == b'aGVsbG8'
assert base64.b64decode(b'aGVsbG8', padded=False, canonical=True) == b'hello'
assert murmur3_32(b'hello') == 0x248BFA47
assert xxh3_64(b'') == 0x2D06800538D394C2
assert xxh3_128_batch([b'hello', b'world']) == [
    0xB5E9C1AD071B3E7FC779CFAA5E523818,
    0xFA0D38A9B38280D0891E4985BDB2583E,
]

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
cargo bench --bench xxhash
uv sync --group benchmark --no-install-project
uv run --no-project --with . python benchmarks/python_base64.py
uv run --no-project --with . python benchmarks/python_base64.py --into
uv run --no-project --with . python benchmarks/python_base64.py --bytearray-input
uv run --no-project --with . python benchmarks/python_base64.py --memoryview-input
uv run --no-project --with . python benchmarks/python_base64_batch.py
uv run --no-project --with . python benchmarks/python_base64_batch.py --large
uv run --no-project --with . python benchmarks/python_murmur3.py
uv run --no-project --with . python benchmarks/python_murmur3.py --incremental
uv run --no-project --with . python benchmarks/python_xxhash.py
```

For the same-ISA Windows comparison reported below, rebuild the C baseline with
`$env:CFLAGS='/O2 /arch:AVX2'; cargo clean -p xxhash-c-sys; cargo bench --bench xxhash`.

## Base64: Rust

| Alphabet | Input | Operation | hashcodecs | `base64` | `base64-turbo` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 1 KiB | encode | **24.78 GiB/s** | 4.52 GiB/s | 23.53 GiB/s |
|  | 1 KiB | decode | **16.77 GiB/s** | 3.51 GiB/s | 11.19 GiB/s |
|  | 4 KiB | encode | **32.23 GiB/s** | 4.81 GiB/s | 30.49 GiB/s |
|  | 4 KiB | decode | **22.82 GiB/s** | 3.85 GiB/s | 13.73 GiB/s |
|  | 1 MiB | encode | **36.40 GiB/s** | 4.50 GiB/s | 35.35 GiB/s |
|  | 1 MiB | decode | **27.06 GiB/s** | 3.54 GiB/s | 13.20 GiB/s |
|  | 8 MiB | encode | 16.19 GiB/s | 4.81 GiB/s | **29.01 GiB/s** |
|  | 8 MiB | decode | **15.90 GiB/s** | 3.85 GiB/s | 12.52 GiB/s |
| URL-safe | 1 KiB | encode | **24.69 GiB/s** | 4.32 GiB/s | 22.66 GiB/s |
|  | 1 KiB | decode | **16.52 GiB/s** | 3.51 GiB/s | 11.55 GiB/s |
|  | 4 KiB | encode | 31.48 GiB/s | 5.08 GiB/s | **31.52 GiB/s** |
|  | 4 KiB | decode | **21.95 GiB/s** | 3.71 GiB/s | 13.74 GiB/s |
|  | 1 MiB | encode | 35.83 GiB/s | 4.53 GiB/s | **35.87 GiB/s** |
|  | 1 MiB | decode | **24.00 GiB/s** | 3.59 GiB/s | 13.78 GiB/s |
|  | 8 MiB | encode | 15.84 GiB/s | 5.08 GiB/s | **29.22 GiB/s** |
|  | 8 MiB | decode | **16.20 GiB/s** | 3.71 GiB/s | 12.34 GiB/s |

## MurmurHash3: Rust

| Variant | Input | hashcodecs | `murmur3` | `murmurs` | `fastmurmur3` | `mm3h` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| x86 32-bit | 1 KiB | 3.86 GiB/s | 2.25 GiB/s | 3.67 GiB/s | n/a | **3.93 GiB/s** |
|  | 4 KiB | **3.77 GiB/s** | 2.26 GiB/s | 3.51 GiB/s | n/a | 3.74 GiB/s |
|  | 1 MiB | 3.74 GiB/s | 2.25 GiB/s | 3.46 GiB/s | n/a | **3.79 GiB/s** |
|  | 8 MiB | **3.77 GiB/s** | 2.26 GiB/s | 3.51 GiB/s | n/a | 3.74 GiB/s |
| x86 128-bit | 1 KiB | **7.82 GiB/s** | 4.14 GiB/s | 7.28 GiB/s | n/a | n/a |
|  | 4 KiB | **8.11 GiB/s** | 4.37 GiB/s | 7.41 GiB/s | n/a | n/a |
|  | 1 MiB | **8.38 GiB/s** | 4.35 GiB/s | 7.57 GiB/s | n/a | n/a |
|  | 8 MiB | **8.37 GiB/s** | 4.37 GiB/s | 7.38 GiB/s | n/a | n/a |
| x64 128-bit | 1 KiB | **9.17 GiB/s** | 5.60 GiB/s | 8.18 GiB/s | 8.70 GiB/s | 7.99 GiB/s |
|  | 4 KiB | **9.29 GiB/s** | 6.01 GiB/s | 8.11 GiB/s | 8.66 GiB/s | 8.01 GiB/s |
|  | 1 MiB | **9.31 GiB/s** | 6.04 GiB/s | 8.22 GiB/s | 8.68 GiB/s | 8.19 GiB/s |
|  | 8 MiB | **9.48 GiB/s** | 5.95 GiB/s | 8.19 GiB/s | 8.68 GiB/s | 7.91 GiB/s |

## XXH3: Rust

`upstream C` is xxHash 0.8.3 built through `xxhash-c-sys` with AVX2 enabled, matching the backend selected by hashcodecs on the benchmark host. Batch results hash 32 equal-size inputs and include result-vector allocation.

| Variant | Input | hashcodecs | upstream C | Speedup |
| --- | ---: | ---: | ---: | ---: |
| XXH3-64 | 64 B | **32.00 GiB/s** | 26.15 GiB/s | **1.22x** |
|  | 1 KiB | **25.65 GiB/s** | 24.20 GiB/s | **1.06x** |
|  | 4 KiB | **53.75 GiB/s** | 36.57 GiB/s | **1.47x** |
|  | 1 MiB | **73.91 GiB/s** | 45.94 GiB/s | **1.61x** |
|  | 8 MiB | **44.83 GiB/s** | 36.93 GiB/s | **1.21x** |
| XXH3-128 | 64 B | **9.26 GiB/s** | 7.43 GiB/s | **1.25x** |
|  | 1 KiB | **27.32 GiB/s** | 20.50 GiB/s | **1.33x** |
|  | 4 KiB | **49.41 GiB/s** | 35.34 GiB/s | **1.40x** |
|  | 1 MiB | **73.72 GiB/s** | 43.49 GiB/s | **1.69x** |
|  | 8 MiB | **42.41 GiB/s** | 36.94 GiB/s | **1.15x** |

| Batch variant | Item | hashcodecs | upstream C loop | Speedup |
| --- | ---: | ---: | ---: | ---: |
| XXH3-64 | 64 B | **24.94 GiB/s** | 18.93 GiB/s | **1.32x** |
|  | 1 KiB | **65.89 GiB/s** | 24.84 GiB/s | **2.65x** |
|  | 4 KiB | **82.72 GiB/s** | 34.80 GiB/s | **2.38x** |
|  | 1 MiB | **24.42 GiB/s** | 11.99 GiB/s | **2.04x** |
| XXH3-128 | 64 B | **8.71 GiB/s** | 6.98 GiB/s | **1.25x** |
|  | 1 KiB | **59.47 GiB/s** | 19.81 GiB/s | **3.00x** |
|  | 4 KiB | **79.72 GiB/s** | 34.38 GiB/s | **2.32x** |
|  | 1 MiB | **25.11 GiB/s** | 12.06 GiB/s | **2.08x** |

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

| Alphabet | Input | Operation | hashcodecs | CPython `base64` | `pybase64` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 1 KiB | encode | **9.41 GiB/s** | 0.44 GiB/s | 5.02 GiB/s |
|  | 1 KiB | decode | **7.50 GiB/s** | 0.93 GiB/s | 3.09 GiB/s |
|  | 4 KiB | encode | **21.04 GiB/s** | 0.46 GiB/s | 13.11 GiB/s |
|  | 4 KiB | decode | **15.21 GiB/s** | 1.08 GiB/s | 8.01 GiB/s |
|  | 1 MiB | encode | **2.85 GiB/s** | 0.40 GiB/s | 2.68 GiB/s |
|  | 1 MiB | decode | **3.51 GiB/s** | 0.84 GiB/s | 3.26 GiB/s |
|  | 8 MiB | encode | **3.12 GiB/s** | 0.42 GiB/s | 2.91 GiB/s |
|  | 8 MiB | decode | **3.79 GiB/s** | 0.90 GiB/s | 3.57 GiB/s |
| URL-safe | 1 KiB | encode | **9.42 GiB/s** | 0.37 GiB/s | 0.96 GiB/s |
|  | 1 KiB | decode | **6.29 GiB/s** | 0.47 GiB/s | 1.13 GiB/s |
|  | 4 KiB | encode | **20.73 GiB/s** | 0.39 GiB/s | 1.12 GiB/s |
|  | 4 KiB | decode | **12.66 GiB/s** | 0.69 GiB/s | 1.47 GiB/s |
|  | 1 MiB | encode | **2.86 GiB/s** | 0.33 GiB/s | 0.84 GiB/s |
|  | 1 MiB | decode | **3.49 GiB/s** | 0.60 GiB/s | 1.34 GiB/s |
|  | 8 MiB | encode | **3.07 GiB/s** | 0.34 GiB/s | 0.84 GiB/s |
|  | 8 MiB | decode | **3.67 GiB/s** | 0.59 GiB/s | 1.38 GiB/s |

## MurmurHash3: Python

| Variant | API | Input | hashcodecs | `mmh3` |
| --- | --- | --- | ---: | ---: |
| x86 32-bit | one-shot | 1 KiB | 3.19 GiB/s | **3.22 GiB/s** |
|  |  | 4 KiB | **3.57 GiB/s** | 3.50 GiB/s |
|  |  | 1 MiB | **3.61 GiB/s** | 3.60 GiB/s |
|  |  | 8 MiB | **3.74 GiB/s** | 3.57 GiB/s |
|  | incremental | 1 KiB | 2.65 GiB/s | **2.86 GiB/s** |
|  |  | 4 KiB | 3.43 GiB/s | **3.48 GiB/s** |
|  |  | 1 MiB | **3.99 GiB/s** | 3.84 GiB/s |
|  |  | 8 MiB | **3.99 GiB/s** | 3.84 GiB/s |
| x86 128-bit | one-shot | 1 KiB | 5.26 GiB/s | **6.16 GiB/s** |
|  |  | 4 KiB | 7.62 GiB/s | **7.73 GiB/s** |
|  |  | 1 MiB | **8.61 GiB/s** | 8.34 GiB/s |
|  |  | 8 MiB | 7.83 GiB/s | **7.99 GiB/s** |
|  | incremental | 1 KiB | **4.01 GiB/s** | 0.69 GiB/s |
|  |  | 4 KiB | **7.09 GiB/s** | 0.78 GiB/s |
|  |  | 1 MiB | **9.27 GiB/s** | 0.80 GiB/s |
|  |  | 8 MiB | **9.27 GiB/s** | 0.81 GiB/s |
| x64 128-bit | one-shot | 1 KiB | 5.36 GiB/s | **7.31 GiB/s** |
|  |  | 4 KiB | 8.06 GiB/s | **8.92 GiB/s** |
|  |  | 1 MiB | 9.49 GiB/s | **9.64 GiB/s** |
|  |  | 8 MiB | **9.45 GiB/s** | 9.12 GiB/s |
|  | incremental | 1 KiB | 4.34 GiB/s | **5.32 GiB/s** |
|  |  | 4 KiB | 7.79 GiB/s | **8.05 GiB/s** |
|  |  | 1 MiB | **10.11 GiB/s** | 9.36 GiB/s |
|  |  | 8 MiB | **10.10 GiB/s** | 8.20 GiB/s |

## XXH3: Python

The batch comparison uses one native `hashcodecs` call versus a loop over the upstream `xxhash` extension.

| Variant | Input | hashcodecs | `xxhash` | Speedup |
| --- | ---: | ---: | ---: | ---: |
| XXH3-64 | 1 KiB | **13.41 GiB/s** | 13.40 GiB/s | 1.00x |
|  | 4 KiB | **33.30 GiB/s** | 29.06 GiB/s | **1.15x** |
|  | 1 MiB | **77.72 GiB/s** | 46.52 GiB/s | **1.67x** |
|  | 8 MiB | **46.24 GiB/s** | 38.84 GiB/s | **1.19x** |
| XXH3-128 | 1 KiB | 8.82 GiB/s | **9.27 GiB/s** | 0.95x |
|  | 4 KiB | **26.79 GiB/s** | 22.17 GiB/s | **1.21x** |
|  | 1 MiB | **76.94 GiB/s** | 48.47 GiB/s | **1.59x** |
|  | 8 MiB | **46.72 GiB/s** | 38.27 GiB/s | **1.22x** |

| Batch variant | Item | hashcodecs | `xxhash` loop | Speedup |
| --- | ---: | ---: | ---: | ---: |
| XXH3-64 | 64 B | **2.78 GiB/s** | 2.15 GiB/s | **1.30x** |
|  | 1 KiB | **27.57 GiB/s** | 14.55 GiB/s | **1.89x** |
|  | 4 KiB | **57.45 GiB/s** | 30.50 GiB/s | **1.88x** |
|  | 1 MiB | **36.18 GiB/s** | 18.39 GiB/s | **1.97x** |
| XXH3-128 | 64 B | **1.10 GiB/s** | 1.01 GiB/s | **1.09x** |
|  | 1 KiB | **14.81 GiB/s** | 9.87 GiB/s | **1.50x** |
|  | 4 KiB | **39.90 GiB/s** | 23.87 GiB/s | **1.67x** |
|  | 1 MiB | **36.29 GiB/s** | 13.75 GiB/s | **2.64x** |

Reusable-buffer and mutable-input results are available in [BENCHMARK.md](https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md).

## SIMD References

The SIMD implementation follows the approach described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605), with AVX-512 VBMI and AArch64 NEON backends selected automatically when available.
