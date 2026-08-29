# Architecture

`hashcodecs` is a Rust library with a substantial CPython compatibility layer and a small Python facade. The
design keeps
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
| `src/base64.rs`, `src/base64/` | Base64 façade; encode/decode operations; alphabets; output ownership; dispatch; scalar and SIMD kernels. |
| `src/murmur3.rs`, `src/murmur3/` | MurmurHash3 façade; x86-32, x86-128, and x64-128 variants; incremental buffering; dispatch. |
| `src/xxhash.rs`, `src/xxhash/` | XXH3 façade; short-input formulas; long-input accumulation; batching; scalar and SIMD kernels. |
| `src/bindings/mod.rs` | CPython extension composition root; it only assembles public functions and classes. |
| `src/bindings/arguments.rs`, `objects.rs`, `runtime.rs` | Shared CPython parsing, object access, function registration, and GIL policy. |
| `src/bindings/{base64,murmur3,xxhash}/` | Algorithm-specific CPython adapters. |
| `hashcodecs/` | Typed Python facade and public module organization. |
| `benches/`, `benchmarks/` | Rust and Python throughput measurements. |
| `tests/`, `fuzz/` | Python compatibility tests, differential fuzzing, and safety validation. |

## Dependency Rules

The crate uses layered modules rather than a workspace of small crates. The boundaries are:

- public algorithm façades document and reexport the stable Rust API;
- functional modules own validation, scalar behavior, and algorithm state;
- dispatch modules depend on the shared CPU capability snapshot and select interchangeable kernels;
- architecture-specific kernels never depend on Python bindings;
- algorithm-specific Python adapters depend on the Rust APIs and shared binding policies;
- `bindings/mod.rs` is a composition root and contains no parsing, buffer, or execution policy.

Shared state machines own their invariants. For example, `murmur3/incremental.rs` keeps the pending block and its
length together, so each incremental hasher cannot represent an inconsistent tail. At the CPython boundary,
`objects.rs` contains raw object access, `buffer.rs` owns borrowing and copying decisions, `arguments.rs` owns
call-shape parsing, and `runtime.rs` owns GIL-detachment and native function registration.

New code should depend toward these shared policies instead of reaching sideways into another algorithm adapter.
Tests, Miri checks, and Kani proofs live in separate modules next to the functionality they cover.

## Functional Module Shape

The crate uses feature-first modules. Each algorithm keeps its public façade at `src/<algorithm>.rs` and its
implementation under `src/<algorithm>/`. The implementation follows the algorithm's main change axis:

| Algorithm | Main module boundary | Reason |
| --- | --- | --- |
| Base64 | `encode` and `decode`, then ISA kernel | Encoding and decoding have separate validation, sizing, and kernel flows; flat ISA files keep hot-path ownership visible. |
| MurmurHash3 | `x86_32`, `x86_128`, and `x64_128` | Each canonical variant owns one-shot hashing, incremental state, tail handling, and finalization. |
| XXH3 | `short`, `long`, and `batch` | XXH3-64 and XXH3-128 share primitives and the long-input accumulator. |

The modules share dependency direction and visibility rules. They do not share an identical file template. Hot
paths use direct calls and static dispatch; module boundaries do not add runtime traits or heap allocation.

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

`base64.rs` reexports the public operations and error type. `alphabet.rs` owns lookup tables, `output.rs` owns
allocation initialization, and the `encode` and `decode` modules own their operation flows. Their architecture
kernels are flat operation children such as `encode/avx2.rs`, `encode/ssse3.rs`, `decode/sse41.rs`, and
`decode/aarch64.rs`; `decode/x86_contracts.rs` holds contracts shared by multiple x86 decoders. The runtime backend
prefers AVX-512 VBMI, AVX2, SSE4.1, SSSE3, NEON, then scalar when supported. Scalar code handles short inputs and
all tails.

The API has three output models:

- allocating functions return a new byte string or vector;
- `*_into` functions write into caller-managed storage;
- allocating batch functions discard partial result lists on failure;
- Base64 reusable-output batches are intentionally fail-fast and non-transactional;
- the XXH3 binding validates and stabilizes every packed-batch input before it mutates the destination.

The Python Base64 binding sends strict input to the SIMD core without constructing a discarded Python exception.
For lenient input, it keeps the MIME whitespace path on normalized SIMD input and decodes other ignored bytes into
the final Python object or reusable buffer. The native state machine follows the padding behavior of each supported
CPython patch series. It calls `binascii` only when malformed input needs CPython's exact exception.

Large aligned x86 encoding may use non-temporal stores after the input exceeds the detected private-cache working
set. Smaller work stays on ordinary cached stores.

### MurmurHash3

The `x86_32`, `x86_128`, and `x64_128` modules each own one-shot calls, incremental state, scalar block mixing,
tail handling, and finalization. All variants use `incremental.rs` for pending blocks and `primitives.rs` for
little-endian loads and finalizers. One-shot calls choose scalar, SSE4.1, or AVX2 using explicit size thresholds.

### XXH3

`hash.rs` selects the canonical length class, `short.rs` contains the 0-240-byte formulas, and `long.rs` owns
secret initialization, scheduling, accumulation, and merging. XXH3-64 and XXH3-128 share these modules.
`long/{aarch64,avx2,avx512,ssse3}.rs` contains the ISA kernels beside the scalar long-input flow and its CPU
selection. Only long inputs enter those kernels.

The AVX2 one-shot kernel splits each full 1,024-byte block across four accumulator chains. It also splits tails
that contain at least four stripes, including the final overlapping stripe. The kernel reduces the chains before
each block scramble and before the final merge.

Native batches reuse the initialized secret and process inputs in groups of up to four. Two to four equal-size
inputs longer than 240 bytes use an AVX2 batch accumulator when available; single items and mixed sizes use the
regular paths.
Python exposes two result models:

- `xxh3_*_batch` returns ergonomic `list[int]` results;
- `xxh3_*_batch_into` writes packed little-endian digests into one reusable `bytearray` and returns bytes written.

The binding validates capacity and stabilizes every input before it mutates the destination. For small stable
batches, it writes each infallible hash result to the packed output. For detached large batches and arbitrary
exporters, it retains temporary results until hashing finishes; this fallback lets callers use the output bytearray
as an input.

## CPython Boundary

The extension uses version-specific CPython APIs rather than the stable ABI. Native functions are registered as
fast callbacks, with shared parsers enforcing Python-compatible positional and keyword behavior.

Algorithm adapters separate execution callbacks from method registration. MurmurHash3 also separates one-shot
callbacks, digest formatting, and incremental Python classes.

Buffer handling follows ownership rather than treating every buffer alike:

- exact `bytes` are borrowed without a copy;
- exact `bytearray` values are borrowed but never used in detached work;
- on GIL-enabled builds, contiguous memoryviews—including small and sliced views—are borrowed while execution remains
  attached to the interpreter;
- full contiguous `bytes` or `bytearray` owners are retained when their data pointer and length match the view, which
  also avoids copies on free-threaded builds;
- other views are flattened into stable bytes when their layout is non-contiguous or stable ownership is required,
  including sliced views on free-threaded builds;
- Base64 encoding still requires C-contiguous input, while hashing and Base64 decoding accept and flatten
  non-contiguous views;
- reusable Base64 batches snapshot only inputs whose memory range overlaps a destination;
- immutable Base64 and XXH3 operations at or above 256 KiB may detach from the GIL;
- immutable MurmurHash3 operations at or above 64 KiB may detach from the GIL;
- mutable storage never crosses a detached region.

Allocating outputs are initialized directly in CPython-owned memory. Reusable-output APIs validate capacity before
writing and preserve bytes beyond the returned length.

## Python Package

The typed `_hashcodecs.pyi` declaration is the canonical Python API description. It drives the public modules,
package exports, module stubs, native text signatures and docstrings, and API-reference member lists through
`tools/generate_api_metadata.py`. The generated `base64.py`, `murmur3.py`, and `xxhash.py` modules organize exports
without adding per-call wrappers. `py.typed` makes the declarations visible to type checkers.

Wheel tests execute the installed package, while coverage paths map that installed location back to the root source
package.

The 100% Rust line-coverage gate continues to measure the Rust core without default features. Separate Linux
coverage jobs build instrumented CPython extensions for Python 3.10, Python 3.12, and free-threaded Python 3.15,
run both the Python suite and Rust binding unit tests, and merge the binding-layer Rust coverage under a
`rust-bindings` flag. The feature-gated binding layer remains outside the core percentage, the sanitizer jobs, Miri
interpretation, and Kani proofs.

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
