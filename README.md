# hashcodecs

[![CI](https://img.shields.io/github/actions/workflow/status/kozistr/hashcodecs-rs/ci.yml?branch=main&style=for-the-badge&logo=github)](https://github.com/kozistr/hashcodecs-rs/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/readthedocs/hashcodecs-rs?style=for-the-badge&logo=readthedocs)](https://hashcodecs-rs.readthedocs.io/en/latest/?badge=latest)
[![PyPI](https://img.shields.io/pypi/v/hashcodecs?style=for-the-badge&logo=pypi)](https://pypi.org/project/hashcodecs/)
[![Python](https://img.shields.io/pypi/pyversions/hashcodecs?style=for-the-badge&logo=python)](https://pypi.org/project/hashcodecs/)
[![Codecov](https://img.shields.io/codecov/c/github/kozistr/hashcodecs-rs?style=for-the-badge&logo=codecov)](https://codecov.io/gh/kozistr/hashcodecs-rs)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-brightgreen?style=for-the-badge)](https://github.com/kozistr/hashcodecs-rs#license)
![Total Downloads](https://img.shields.io/pepy/dt/hashcodecs?style=for-the-badge&label=Total%20Downloads)
![Monthly Downloads](https://img.shields.io/pypi/dm/hashcodecs?style=for-the-badge&label=Monthly%20downloads)

SIMD-accelerated Base64, MurmurHash3, and XXH3 for Python and Rust.

Move byte-heavy work into Rust without changing your Python inputs. `hashcodecs` accepts `bytes`, `bytearray`, and
`memoryview`, selects the best available SIMD backend, and exposes batch and reusable-buffer APIs.

## Features
- Base64 encode and decode with standard, URL-safe, padded, unpadded, wrapped, and canonical modes.
- MurmurHash3 x86-32, x86-128, and x64-128 with one-shot and incremental APIs.
- Bit-for-bit compatible XXH3-64 and XXH3-128 with one-shot and native batch APIs.
- Caller-managed `*_into` outputs for allocation-sensitive workloads.
- Runtime dispatch across AVX-512, AVX2, SSE4.1, SSSE3, NEON, and scalar implementations where applicable.
- Direct CPython buffer handling for `bytes`, `bytearray`, and `memoryview` inputs.
- Install wheels for CPython 3.10 through 3.15 and free-threaded CPython
  3.14t and 3.15t on Linux, macOS, and Windows.

## Citation

If you use `hashcodecs`, cite [CITATION.cff](CITATION.cff).

## Installation

```sh
pip3 install hashcodecs
```

## Performance snapshot

On the benchmark host, `hashcodecs.xxh3_64` processes a 1 MiB input at 78.83 GiB/s. The operator pinned one logical
CPU and measured hashcodecs alone. At 256 B items in batches of 64, the Base64 batch API reaches 8.56 GiB/s for encode and 7.60 GiB/s for decode. The per-item loop reaches 4.41 and 3.39 GiB/s. Read the [benchmark details](BENCHMARK.md) and [raw results](docs/benchmarks/results.csv).

## Python

The Base64 module follows familiar Python conventions while adding explicit padding, canonical validation, batch,
and reusable-buffer operations.

```python
import hashcodecs.base64 as base64
from hashcodecs import murmur3_32, xxh3_64, xxh3_128_batch

assert base64.b64encode(b'hello') == b'aGVsbG8='
assert base64.b64decode(b'aGVsbG8=') == b'hello'
assert base64.urlsafe_b64encode(b'hello', padded=False) == b'aGVsbG8'

assert murmur3_32(b'hello') == 0x248BFA47
assert xxh3_64(b'') == 0x2D06800538D394C2
assert xxh3_128_batch([b'hello', b'world']) == [
    0xB5E9C1AD071B3E7FC779CFAA5E523818,
    0xFA0D38A9B38280D0891E4985BDB2583E,
]
```

### Reusable outputs

`*_into` functions write into caller-managed `bytearray` storage and return the number of bytes written. XXH3 batch
outputs are packed little-endian digests.

```python
import hashcodecs.base64 as base64
from hashcodecs import xxh3_64_batch_into

payload = b'hello'
encoded = bytearray(4 * ((len(payload) + 2) // 3))
encoded_len = base64.b64encode_into(payload, encoded)
assert encoded[:encoded_len] == b'aGVsbG8='

hashes = bytearray(2 * 8)
written = xxh3_64_batch_into([b'hello', b'world'], hashes, seed=42)
assert written == 16
```

### Incremental hashing

```python
from hashcodecs import murmur3_x64_128

hasher = murmur3_x64_128(seed=42)
hasher.update(b'hello')
snapshot = hasher.copy()
hasher.update(b' world')

assert snapshot.hexdigest() == snapshot.digest().hex()
assert hasher.digest() != snapshot.digest()
```

## Rust

The Rust API exposes the same core algorithms without the Python binding layer.

```rust
let encoded = hashcodecs::b64encode(b"hello");
assert_eq!(encoded, "aGVsbG8=");

let mut output = [0_u8; 8];
let written = hashcodecs::b64encode_into(b"hello", &mut output).unwrap();
assert_eq!(&output[..written], b"aGVsbG8=");

assert_eq!(hashcodecs::murmur3_x86_32(b"hello", 0), 0x248b_fa47);
assert_eq!(hashcodecs::xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
```

## Architecture

The Rust core owns algorithm behavior and SIMD dispatch. A thin CPython layer handles argument parsing, buffer
ownership, reusable outputs, and GIL decisions; root-level Python modules provide typed public exports without
adding per-call wrappers.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the module layout, dispatch model, algorithm data flows,
CPython boundary, and safety invariants.

## Benchmarks

Run the suite on Windows 10 x64 with an Intel Core Ultra 7 265K. Pin one logical CPU and run each case in one thread.
Collect 50 Rust samples and 15 Python samples. Higher throughput wins.

### Base64: Rust

[![Rust Base64 throughput](docs/benchmarks/base64-rust.svg)](docs/benchmarks/base64-rust.svg)

### MurmurHash3: Rust

[![Rust MurmurHash3 throughput](docs/benchmarks/murmur3-rust.svg)](docs/benchmarks/murmur3-rust.svg)

### XXH3: Rust

Link hashcodecs with xxHash 0.8.3 through `xxhash-c-sys`. Build the C baseline with AVX2. Batch cases pass 32
equal-size inputs and include result-vector allocation.

[![Rust XXH3 throughput](docs/benchmarks/xxh3-rust.svg)](docs/benchmarks/xxh3-rust.svg)

### Base64: Python

Pass `bytes` to Rust without an input copy. Python decoding uses `validate=True`.

[![Python Base64 throughput](docs/benchmarks/base64-python.svg)](docs/benchmarks/base64-python.svg)

### MurmurHash3: Python

[![Python MurmurHash3 throughput](docs/benchmarks/murmur3-python.svg)](docs/benchmarks/murmur3-python.svg)

### XXH3: Python

Pass 32 equal-size inputs to each batch case. Compare one native hashcodecs call with a loop over the upstream
`xxhash` extension.

[![Python XXH3 throughput](docs/benchmarks/xxh3-python.svg)](docs/benchmarks/xxh3-python.svg)

Read the focused cases, commands, and values in [BENCHMARK.md](BENCHMARK.md). Read raw chart values in
[docs/benchmarks/results.csv](docs/benchmarks/results.csv).

### Reproduce

Comparison crates and Python packages are development-only dependencies and are not included in consumer builds.

```sh
cargo bench --bench base64
cargo bench --bench murmur3
cargo bench --bench xxhash

uv sync --group benchmark --no-install-project
uv run --no-project --with . python benchmarks/python_base64.py
uv run --no-project --with . python benchmarks/python_base64_batch.py
uv run --no-project --with . python benchmarks/python_murmur3.py
uv run --no-project --with . python benchmarks/python_xxhash.py
```

The Python benchmarks expose focused modes such as `--into`, `--bytearray-input`, `--memoryview-input`,
`--incremental`, `--large`, and `--hashcodecs-only`. Use `--help` on a benchmark script for its supported modes.

For the same-ISA Windows XXH3 comparison shown above, rebuild the C baseline with:

```powershell
$env:CFLAGS='/O2 /arch:AVX2'
cargo clean -p xxhash-c-sys
cargo bench --bench xxhash
```

## Development

Build the Python wheel and source distribution:

```sh
uv build
```

Run the primary local checks:

```sh
cargo fmt --check
cargo clippy --all-targets --features python -- -D warnings
cargo test --features python
uv run --frozen --no-sync ruff check . --no-cache
uv run --frozen --no-sync ruff format --check .
uv run --frozen --no-sync pytest tests --cov=hashcodecs --cov-branch --cov-fail-under=100
```

Optimized paths are also checked with differential fuzzing, Kani, strict-provenance Miri, AddressSanitizer, and
MemorySanitizer in CI.

## References

The Base64 SIMD implementation follows the approach described in
[Faster Base64 Encoding and Decoding using AVX2 Instructions](https://arxiv.org/abs/1704.00605), extended with
runtime-selected AVX-512 VBMI and AArch64 NEON backends.

## License

Licensed under either of the following, at your option:

- [Apache License 2.0](LICENSE)
- [MIT License](LICENSE-MIT)
