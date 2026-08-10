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
- Every MurmurHash3 variant uses runtime-dispatched AVX2 or SSE4.1 pre-mixing where its measured crossover beats the scalar loop. The x64 128-bit AVX2 kernel also selects BMI2 rotations when available. Ordered state transitions remain reference-compatible, and every variant has a portable scalar fallback.
- The Python Base64 surface follows the familiar `base64` names. Use `import hashcodecs.base64 as base64` when replacing standard Base64 calls.
- Python 3.15 Base64 options are supported, including unpadded and wrapped encoding plus configurable padding, ignored characters, and canonical decoding.
- Python callers can encode or decode an ordered list of independent values in one native call through the `*_batch` APIs. A batch uses one alphabet and validation setting, stops at the first error, and returns `list[bytes]` in input order.
- Python callers can reuse a `bytearray` through the `*_into` APIs when returned `bytes` ownership is unnecessary. Mutable inputs are borrowed without a copy and keep the GIL to prevent concurrent mutation.
- Large immutable Python inputs release the GIL. Strict decoding and valid default-mode decoding write directly into the returned `bytes`; malformed default-mode input uses a CPython-compatible lenient fallback.
- Batch processing is single-threaded and uses memory proportional to its results. Items below 64 KiB retain the GIL; each larger immutable item releases and reacquires it independently. Callers must not mutate the input list concurrently.

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
uv run --no-project python benchmarks/python_base64.py
uv run --no-project python benchmarks/python_base64.py --into
uv run --no-project python benchmarks/python_base64.py --bytearray-input
uv run --no-project python benchmarks/python_base64_batch.py
uv run --no-project python benchmarks/python_murmur3.py
uv run --no-project python benchmarks/python_murmur3.py --incremental
```

To time only this implementation:

```sh
cargo bench --bench base64 -- hashcodecs
cargo bench --bench murmur3 -- hashcodecs
uv run --no-project python benchmarks/python_base64.py --hashcodecs-only
uv run --no-project python benchmarks/python_murmur3.py --hashcodecs-only
uv run --no-project python benchmarks/python_murmur3.py --incremental --hashcodecs-only
uv run --no-project python benchmarks/python_murmur3.py --bytearray-input --hashcodecs-only
uv run --no-project python benchmarks/python_murmur3.py --incremental --bytearray-input --hashcodecs-only
```

## Base64: Rust

| Alphabet | Input | Operation | hashcodecs | `base64` | `base64-turbo` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | **19.06 GiB/s** | 5.40 GiB/s | 16.99 GiB/s |
|  | 4 KiB | decode | **25.18 GiB/s** | 4.05 GiB/s | 15.76 GiB/s |
|  | 1 MiB | encode | **39.10 GiB/s** | 4.80 GiB/s | 18.22 GiB/s |
|  | 1 MiB | decode | **28.54 GiB/s** | 3.81 GiB/s | 16.50 GiB/s |
|  | 32 MiB | encode | **11.07 GiB/s** | 3.01 GiB/s | 10.31 GiB/s |
|  | 32 MiB | decode | **10.74 GiB/s** | 3.12 GiB/s | 9.86 GiB/s |
| URL-safe | 4 KiB | encode | **19.04 GiB/s** | 5.40 GiB/s | 17.01 GiB/s |
|  | 4 KiB | decode | **23.89 GiB/s** | 4.05 GiB/s | 15.75 GiB/s |
|  | 1 MiB | encode | **39.13 GiB/s** | 4.75 GiB/s | 18.21 GiB/s |
|  | 1 MiB | decode | **26.98 GiB/s** | 3.80 GiB/s | 16.06 GiB/s |
|  | 32 MiB | encode | **11.07 GiB/s** | 3.01 GiB/s | 10.30 GiB/s |
|  | 32 MiB | decode | **10.79 GiB/s** | 3.12 GiB/s | 9.75 GiB/s |

## MurmurHash3: Rust

| Variant | Input | hashcodecs | `murmur3` | `murmurs` | `fastmurmur3` | `mm3h` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| x86 32-bit | 4 KiB | **4.03 GiB/s** | 2.44 GiB/s | 3.82 GiB/s | n/a | 4.02 GiB/s |
|  | 1 MiB | **3.99 GiB/s** | 2.43 GiB/s | 3.79 GiB/s | n/a | 3.97 GiB/s |
|  | 32 MiB | **3.98 GiB/s** | 2.42 GiB/s | 3.65 GiB/s | n/a | 3.81 GiB/s |
| x86 128-bit | 4 KiB | **8.90 GiB/s** | 4.66 GiB/s | 8.07 GiB/s | n/a | n/a |
|  | 1 MiB | **9.21 GiB/s** | 4.71 GiB/s | 8.22 GiB/s | n/a | n/a |
|  | 32 MiB | **9.12 GiB/s** | 4.55 GiB/s | 5.73 GiB/s | n/a | n/a |
| x64 128-bit | 4 KiB | **10.08 GiB/s** | 6.38 GiB/s | 8.85 GiB/s | 9.41 GiB/s | 8.91 GiB/s |
|  | 1 MiB | **10.11 GiB/s** | 6.44 GiB/s | 8.87 GiB/s | 9.37 GiB/s | 8.88 GiB/s |
|  | 32 MiB | **9.54 GiB/s** | 5.83 GiB/s | 6.39 GiB/s | 6.99 GiB/s | 6.41 GiB/s |

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

| Alphabet | Input | Operation | hashcodecs | CPython `base64` | `pybase64` |
| --- | --- | --- | ---: | ---: | ---: |
| Standard | 4 KiB | encode | 13.63 GiB/s | 0.49 GiB/s | **13.92 GiB/s** |
|  | 4 KiB | decode | **16.75 GiB/s** | 1.15 GiB/s | 8.36 GiB/s |
|  | 1 MiB | encode | **3.60 GiB/s** | 0.47 GiB/s | 3.39 GiB/s |
|  | 1 MiB | decode | **4.51 GiB/s** | 0.90 GiB/s | 4.41 GiB/s |
|  | 32 MiB | encode | **2.95 GiB/s** | 0.45 GiB/s | 2.93 GiB/s |
|  | 32 MiB | decode | **3.65 GiB/s** | 0.96 GiB/s | 3.55 GiB/s |
| URL-safe | 4 KiB | encode | **12.96 GiB/s** | 0.41 GiB/s | 1.19 GiB/s |
|  | 4 KiB | decode | **11.39 GiB/s** | 0.78 GiB/s | 1.55 GiB/s |
|  | 1 MiB | encode | **3.61 GiB/s** | 0.34 GiB/s | 0.91 GiB/s |
|  | 1 MiB | decode | **4.23 GiB/s** | 0.62 GiB/s | 1.48 GiB/s |
|  | 32 MiB | encode | **2.94 GiB/s** | 0.35 GiB/s | 0.90 GiB/s |
|  | 32 MiB | decode | **3.61 GiB/s** | 0.62 GiB/s | 1.47 GiB/s |

## MurmurHash3: Python

| Variant | API | Input | hashcodecs | `mmh3` |
| --- | --- | --- | ---: | ---: |
| x86 32-bit | one-shot | 4 KiB | **3.80 GiB/s** | 3.72 GiB/s |
|  |  | 1 MiB | **4.00 GiB/s** | 3.84 GiB/s |
|  |  | 32 MiB | **3.99 GiB/s** | 3.68 GiB/s |
|  | incremental | 4 KiB | **3.60 GiB/s** | 3.59 GiB/s |
|  |  | 1 MiB | **4.00 GiB/s** | 3.83 GiB/s |
|  |  | 32 MiB | **3.99 GiB/s** | 3.79 GiB/s |
| x86 128-bit | one-shot | 4 KiB | 8.19 GiB/s | **8.22 GiB/s** |
|  |  | 1 MiB | **9.28 GiB/s** | 8.89 GiB/s |
|  |  | 32 MiB | **9.18 GiB/s** | 6.25 GiB/s |
|  | incremental | 4 KiB | **7.05 GiB/s** | 0.79 GiB/s |
|  |  | 1 MiB | **9.17 GiB/s** | 0.78 GiB/s |
|  |  | 32 MiB | **9.12 GiB/s** | 0.79 GiB/s |
| x64 128-bit | one-shot | 4 KiB | 8.86 GiB/s | **9.48 GiB/s** |
|  |  | 1 MiB | 10.07 GiB/s | **10.25 GiB/s** |
|  |  | 32 MiB | **9.55 GiB/s** | 6.82 GiB/s |
|  | incremental | 4 KiB | 7.57 GiB/s | **8.02 GiB/s** |
|  |  | 1 MiB | **10.07 GiB/s** | 9.18 GiB/s |
|  |  | 32 MiB | **9.57 GiB/s** | 7.44 GiB/s |

Reusable-buffer and mutable-input results are available in [BENCHMARK.md](https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md).

## SIMD References

The SIMD implementation follows the approach described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605), with AVX-512 VBMI and AArch64 NEON backends selected automatically when available.
