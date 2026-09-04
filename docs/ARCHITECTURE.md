# Architecture

`hashcodecs` combines a Rust library, a CPython compatibility layer, and a small Python facade. Separate algorithm,
CPU, and binding layers let maintainers change one without disrupting the others.

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

The scalar implementations set the portability and correctness baseline. Runtime dispatch can select a faster kernel while preserving output formats and public behavior.

## Repository Layout

| Path | Responsibility |
| --- | --- |
| `src/backend.rs` | Process-wide CPU capability detection shared by dispatchers. |
| `src/base64.rs`, `src/base64/` | Base64 public API, operations, alphabets, output buffers, runtime dispatch, and kernels. |
| `src/murmur3.rs`, `src/murmur3/` | MurmurHash3 public API, variants, incremental buffers, and dispatch. |
| `src/xxhash.rs`, `src/xxhash/` | XXH3 public API, length-specific formulas, long-input accumulation, batching, and kernels. |
| `src/bindings.rs` | CPython extension composition root for public functions and classes. |
| `src/bindings/arguments.rs`, `objects.rs`, `runtime.rs` | Shared CPython parsing, object access, function registration, and GIL policy. |
| `src/bindings/{base64,murmur3,xxhash}/` | Algorithm-specific CPython adapters. |
| `hashcodecs/` | Typed Python facade and public module organization. |
| `benches/`, `benchmarks/` | Rust and Python throughput measurements. |
| `tests/`, `fuzz/` | Python compatibility tests, differential fuzzing, and safety validation. |

## Dependency Rules

The crate groups code into layered modules within one crate:

- public algorithm façades document and reexport the stable Rust API;
- functional modules own validation, scalar behavior, and algorithm state;
- dispatch modules depend on the shared CPU capability snapshot and select interchangeable kernels;
- architecture-specific kernels have no dependency on Python bindings;
- algorithm-specific Python adapters depend on the Rust APIs and shared binding policies;
- `bindings.rs` is a composition root and contains no parsing, buffer, or execution policy.

Shared state machines own their invariants. For example, `murmur3/block_buffer.rs` keeps the pending block and its
length together, so each incremental hasher cannot represent an inconsistent tail. At the CPython boundary,
`objects.rs` contains raw object access, `buffer.rs` owns borrowing and copying decisions, `arguments.rs` owns
call-shape parsing, and `runtime.rs` owns GIL-detachment and native function registration.

Route new code through these shared policy modules. Keep algorithm adapters isolated from one another. Tests, Miri
checks, and Kani proofs live in separate modules next to the functionality they cover.

## Functional Module Shape

The crate uses feature-first modules. Each algorithm keeps its public façade at `src/<algorithm>.rs` and its
implementation under `src/<algorithm>/`. The implementation follows the algorithm's main change axis:

| Algorithm | Main module boundary | Reason |
| --- | --- | --- |
| Base64 | `encode` and `decode`, then ISA kernel | Encoding and decoding have separate validation, sizing, and kernel flows; flat ISA files keep hot-path ownership visible. |
| MurmurHash3 | `x86_32`, `x86_128`, and `x64_128` | Each canonical variant owns one-shot hashing, incremental state, tail handling, and finalization. |
| XXH3 | `short_inputs`, `long_inputs`, and `batch` | XXH3-64 and XXH3-128 share primitives and the long-input accumulator. |

The modules share dependency direction and visibility rules, while each algorithm uses a file layout suited to its
implementation. Hot paths use direct calls and static dispatch; module boundaries add no runtime traits or heap allocation.

## Runtime Dispatch

The backend detects CPU capabilities once and caches them in a `OnceLock`. Dispatchers receive a compact capability
value, which keeps feature detection out of individual calls. Explicit checks guard unsupported instructions.

- x86 and x86-64 may expose any combination of SSSE3, SSE4.1, AVX2, AVX-512, AVX-512 VBMI, and BMI2.
- AArch64 may expose NEON.
- Other targets, Kani, and Miri use the scalar baseline.
- Dispatch considers both the available ISA and input size because SIMD setup can cost more than scalar work on small inputs.

## Algorithms

### Base64

`base64.rs` reexports the public operations and error type. `alphabet.rs` owns lookup tables.
`output_buffer.rs` owns allocation initialization. The `encode` and `decode` modules own their operation flows. Their architecture
kernels are flat operation children such as `encode/avx2.rs`, `encode/ssse3.rs`, `decode/sse41.rs`, and
`decode/aarch64.rs`; `decode/x86_contracts.rs` holds contracts shared by multiple x86 decoders. The runtime backend
prefers AVX-512 VBMI, AVX2, SSE4.1, SSSE3, NEON, then scalar when supported. Scalar code handles short inputs and
all tails.

Output allocation and failure behavior vary by API:

- allocating functions return a new byte string or vector;
- `*_into` functions write into caller-managed storage;
- allocating batch functions discard partial result lists on failure;
- Base64 reusable-output batches stop at the first error and retain prior destination writes;
- the XXH3 binding validates and stabilizes all packed-batch inputs before it mutates the destination.

The Python Base64 binding sends strict input to the SIMD core without constructing a discarded Python exception.
For lenient input, it keeps the MIME whitespace path on normalized SIMD input and decodes other ignored bytes into
the final Python object or reusable buffer. The native state machine follows the padding behavior of each supported
CPython patch series. It calls `binascii` for malformed input that needs CPython's exact exception.

Large aligned x86 encoding may use non-temporal stores after the input exceeds the detected private-cache working
set. Smaller work stays on ordinary cached stores.

### MurmurHash3

The `x86_32`, `x86_128`, and `x64_128` modules each own one-shot calls, incremental state, scalar block mixing,
tail handling, and finalization. All variants use `incremental.rs` for pending blocks and `primitives.rs` for
little-endian loads and finalizers. One-shot calls choose scalar, SSE4.1, or AVX2 using explicit size thresholds.

### XXH3

`one_shot.rs` selects the input-length class. `short_inputs.rs` contains the formulas for 0 to 240 bytes.
`long_inputs.rs` owns secret initialization, scheduling, accumulation, and merging. XXH3-64 and XXH3-128 share these modules.
`long_inputs/aarch64.rs`, `long_inputs/avx2.rs`, `long_inputs/avx512.rs`, and `long_inputs/ssse3.rs` contain the ISA kernels.
The scalar long-input flow and backend selection use the same module. These kernels handle inputs longer than 240 bytes.

The AVX2 one-shot kernel splits each full 1,024-byte block across four accumulator chains. It also splits tails
that contain at least four stripes, including the final overlapping stripe. The kernel reduces the chains before
each block scramble and before the final merge.

Native batches reuse the initialized secret and process inputs in groups of up to four. Two to four equal-size
inputs longer than 240 bytes use an AVX2 batch accumulator when available; single items and mixed sizes use the
regular paths.
Python exposes two result models:

- `xxh3_*_batch` returns `list[int]` results;
- `xxh3_*_batch_into` writes packed little-endian digests into one reusable `bytearray` and returns bytes written.

The binding validates capacity and stabilizes all inputs before it mutates the destination. For small stable
batches, it writes each infallible hash result to the packed output. For detached large batches and arbitrary
exporters, it retains temporary results until hashing finishes; this fallback lets callers use the output bytearray
as an input.

## CPython Boundary

The extension uses version-specific CPython APIs instead of the stable ABI. It registers native functions as fast
callbacks, and shared parsers enforce Python-compatible positional and keyword behavior.

Algorithm adapters separate execution callbacks from method registration. MurmurHash3 also separates one-shot
callbacks, digest formatting, and incremental Python classes.

Buffer ownership determines whether the binding borrows or copies an input:

- The binding borrows exact `bytes` without copying them.
- It borrows exact `bytearray` values for work that remains attached to the interpreter.
- On GIL-enabled builds, it borrows contiguous memoryviews, including small and sliced views, while execution remains
  attached to the interpreter.
- It retains a full contiguous `bytes` or `bytearray` owner when its data pointer and length match the view. This
  avoids a copy on free-threaded builds.
- It flattens other views into stable bytes when the layout is non-contiguous or detached work requires stable
  ownership. Sliced views on free-threaded builds follow this path.
- Base64 encoding requires C-contiguous input. Hashing and Base64 decoding accept and flatten non-contiguous views.
- Reusable Base64 batches snapshot inputs whose memory range overlaps the destination.
- The runtime may release the GIL for immutable Base64 and XXH3 inputs of at least 256 KiB.
- The runtime may release the GIL for immutable MurmurHash3 inputs of at least 64 KiB.
- The runtime keeps mutable storage out of detached regions.

The binding initializes allocating outputs in CPython-owned memory. Reusable-output APIs validate capacity before
writing and preserve bytes beyond the returned length.

## Python Package

The typed `_hashcodecs.pyi` declaration is the canonical Python API description. It drives the public modules,
package exports, module stubs, native text signatures and docstrings, and API-reference member lists through
`tools/generate_api_metadata.py`. The generated `base64.py`, `murmur3.py`, and `xxhash.py` modules organize exports
without adding per-call wrappers. Generated Rust schemas live under `generated/rust` and are included by thin binding
modules, so metadata generation never rewrites handwritten Rust source. `py.typed` makes the declarations visible to
type checkers.

Wheel tests execute the installed package. Coverage paths map its installed location back to the root source package.

The Rust core coverage job disables default features and requires 100% line coverage. Three Linux binding-coverage
jobs build instrumented CPython extensions for Python 3.10, Python 3.12, and free-threaded Python 3.15. Each job runs
the Python suite and Rust binding unit tests, then reports merged coverage under the `rust-bindings` flag. Core
coverage, sanitizer jobs, Miri, and Kani exclude the feature-gated binding layer.

## Correctness and Safety

Optimized kernels must remain interchangeable with the scalar baseline. These checks enforce that requirement:

- known-answer and boundary tests;
- differential tests against established Base64, MurmurHash3, and xxHash implementations;
- randomized and fuzz inputs;
- Kani proofs for raw-load and output bounds;
- strict-provenance Miri runs;
- AddressSanitizer and MemorySanitizer jobs;
- Python tests across supported CPython versions and operating systems.

## Performance Work

Benchmarks pin one logical CPU, validate outputs before timing, and cover boundary sizes as well as large inputs.
Rust benchmarks measure the core algorithms. Python benchmarks also measure argument parsing, object allocation,
buffer ownership, and GIL decisions. Reports separate reusable-buffer results from allocating results to show the
API tradeoff.

See [BENCHMARK.md](https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md) and the
[README](https://github.com/kozistr/hashcodecs-rs#development) for commands, host details, charts, and raw result locations.
