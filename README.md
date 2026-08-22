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
- Unsafe paths verified with Kani, strict-provenance Miri, ASan/MSan, and differential fuzzing.

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
from hashcodecs import murmur3_32, murmur3_x64_128, xxh3_64, xxh3_64_batch_into, xxh3_128_batch

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
packed_hashes = bytearray(16)
assert xxh3_64_batch_into([b'hello', b'world'], packed_hashes) == 16

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

For regression checks that do not need fresh competitor measurements, pass
`--hashcodecs-only` to any Python benchmark command.

For the same-ISA Windows comparison reported below, rebuild the C baseline with
`$env:CFLAGS='/O2 /arch:AVX2'; cargo clean -p xxhash-c-sys; cargo bench --bench xxhash`.

## Base64: Rust

[![Rust Base64 throughput](docs/benchmarks/base64-rust.svg)](docs/benchmarks/base64-rust.svg)

## MurmurHash3: Rust

[![Rust MurmurHash3 throughput](docs/benchmarks/murmur3-rust.svg)](docs/benchmarks/murmur3-rust.svg)

## XXH3: Rust

`upstream C` is xxHash 0.8.3 built through `xxhash-c-sys` with AVX2 enabled, matching the backend selected by hashcodecs on the benchmark host. Batch results hash 32 equal-size inputs and include result-vector allocation.

[![Rust XXH3 throughput](docs/benchmarks/xxh3-rust.svg)](docs/benchmarks/xxh3-rust.svg)

## Base64: Python

Python decoding uses `validate=True`, and `hashcodecs` passes `bytes` directly into Rust without an input copy.

[![Python Base64 throughput](docs/benchmarks/base64-python.svg)](docs/benchmarks/base64-python.svg)

## MurmurHash3: Python

[![Python MurmurHash3 throughput](docs/benchmarks/murmur3-python.svg)](docs/benchmarks/murmur3-python.svg)

## XXH3: Python

The batch comparison uses one native `hashcodecs` call versus a loop over the upstream `xxhash` extension.

[![Python XXH3 throughput](docs/benchmarks/xxh3-python.svg)](docs/benchmarks/xxh3-python.svg)

Reusable-buffer and mutable-input charts are available in [BENCHMARK.md](BENCHMARK.md), and performance policies and experiment decisions are tracked in [docs/PERFORMANCE.md](docs/PERFORMANCE.md). Exact chart values are available as [CSV](docs/benchmarks/results.csv).

## SIMD References

The SIMD implementation follows the approach described in [Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605), with AVX-512 VBMI and AArch64 NEON backends selected automatically when available.
