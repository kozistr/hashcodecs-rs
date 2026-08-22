# Architecture

`hashcodecs` is a Rust library with a thin CPython extension and a small Python facade. The design keeps
algorithm correctness, CPU-specific execution, and language bindings separate so each layer can evolve without
changing the others.

## System Shape

```text
Python public modules                 Rust public API
        |                                    |
        v                                    v
CPython argument and buffer layer     algorithm entry points
        |                                    |
        +-------------> runtime dispatch <---+
                              |
                    scalar and SIMD kernels
```

The scalar implementations are the portability and correctness baseline. Runtime dispatch may select a faster
kernel, but never changes output formats or public behavior.

## Repository Layout

| Path | Responsibility |
| --- | --- |
| `src/backend.rs` | Process-wide CPU capability detection shared by dispatchers. |
| `src/base64.rs`, `src/base64/` | Base64 API, validation, scalar implementation, and architecture-specific kernels. |
| `src/murmur3.rs`, `src/murmur3/` | One-shot and incremental MurmurHash3 implementations and dispatch thresholds. |
| `src/xxhash.rs`, `src/xxhash/` | Canonical XXH3-64/128 implementations, long-input SIMD, and native batching. |
| `src/bindings/` | CPython callbacks, argument parsing, buffer ownership, and GIL policy. |
| `hashcodecs/` | Typed Python facade and public module organization. |
| `benches/`, `benchmarks/` | Rust and Python throughput measurements. |
| `tests/`, `fuzz/` | Python compatibility tests, differential fuzzing, and safety validation. |

## Runtime Dispatch

CPU capabilities are detected once and cached in a `OnceLock`. Dispatchers receive a compact capability value
instead of performing feature detection in every call. Unsupported instructions therefore remain behind explicit
checks.

- x86 and x86-64 may expose SSSE3, SSE4.1, AVX2, AVX-512, AVX-512 VBMI, and BMI2 independently.
- AArch64 may expose NEON.
- Other targets, Kani, and Miri use the scalar baseline.
- Dispatch considers both the available ISA and input size because SIMD setup can cost more than scalar work on
  small inputs.

## Algorithms

### Base64

Base64 separates public length and validation rules from encoding and decoding kernels. The runtime backend
prefers AVX-512 VBMI, AVX2, SSE4.1, SSSE3, NEON, then scalar when supported. Scalar code handles short inputs and
all tails.

The API has three output models:

- allocating functions return a new byte string or vector;
- `*_into` functions write into caller-managed storage;
- batch functions parse and validate the full operation before writing results.

Large aligned x86 encoding may use non-temporal stores after the input exceeds the detected private-cache working
set. Smaller work stays on ordinary cached stores.

### MurmurHash3

MurmurHash3 provides x86-32, x86-128, and x64-128 variants. One-shot calls choose scalar, SSE4.1, or AVX2 using
explicit size thresholds. Incremental hashers retain incomplete tails and use the same canonical finalization as
the one-shot functions.

### XXH3

XXH3-64 and XXH3-128 follow the canonical length classes: 0-16, 17-128, 129-240, and long inputs. Only long inputs
enter the accumulation backends; smaller inputs stay on specialized scalar formulas.

Native batches reuse the initialized secret and process inputs in groups of four. Four equal-size inputs longer
than 240 bytes use the AVX2 batch accumulator when available; mixed sizes and remainders use the regular paths.
Python exposes two result models:

- `xxh3_*_batch` returns ergonomic `list[int]` results;
- `xxh3_*_batch_into` writes packed little-endian digests into one reusable `bytearray` and returns bytes written.

The packed path hashes every input before mutating the destination. That makes validation failure atomic and
allows the output bytearray to also appear as an input.

## CPython Boundary

The extension uses version-specific CPython APIs rather than the stable ABI. Native functions are registered as
fast callbacks, with shared parsers enforcing Python-compatible positional and keyword behavior.

Buffer handling follows ownership rather than treating every buffer alike:

- exact `bytes` are borrowed without a copy;
- exact `bytearray` values are borrowed but never used in detached work;
- small or sliced memoryviews are copied;
- full contiguous bytes or bytearray owners can be retained for large memoryviews;
- immutable operations at or above 64 KiB may detach from the GIL;
- mutable storage never crosses a detached region.

Allocating outputs are initialized directly in CPython-owned memory. Reusable-output APIs validate capacity before
writing and preserve bytes beyond the returned length.

## Python Package

The root `hashcodecs/` package is the canonical Python source. `_hashcodecs` contains the native extension surface;
`base64.py`, `murmur3.py`, and `xxhash.py` organize exports without adding per-call wrappers. `_hashcodecs.pyi` and
`py.typed` make the native API visible to type checkers.

Wheel tests execute the installed package, while coverage paths map that installed location back to the root source
package.

## Correctness and Safety

Optimized kernels must remain interchangeable with the scalar baseline. The repository enforces that through:

- known-answer and boundary tests;
- differential tests against established Base64, MurmurHash3, and xxHash implementations;
- randomized and fuzz inputs;
- Kani proofs for raw-load and output bounds;
- strict-provenance Miri runs;
- AddressSanitizer and MemorySanitizer jobs;
- Python tests across supported CPython versions and operating systems.

## Performance Work

Benchmarks pin one logical CPU, validate outputs before timing, and cover boundary sizes as well as large inputs.
Rust benchmarks measure the core algorithms; Python benchmarks additionally measure argument parsing, object
allocation, buffer ownership, and GIL decisions. Reusable-buffer results are kept separate from allocating results
so the API tradeoff remains visible.

Commands, host details, charts, and raw result locations are documented in
[BENCHMARK.md](https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md) and the
[README](https://github.com/kozistr/hashcodecs-rs#development).
