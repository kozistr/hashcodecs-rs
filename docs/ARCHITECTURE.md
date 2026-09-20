# Architecture

`hashcodecs` implements Base64 and the non-cryptographic MurmurHash3 and XXH3 hashes in Rust, with a CPython
extension and typed Python exports. Use this explanation when changing a kernel or binding: it describes the
work each layer avoids and the invariants an optimization must preserve. For measured throughput and reproduction
commands, see [Benchmarks][benchmarks].

## Layers and ownership

```text
Python exports                          Rust public API
      |                                       |
      v                                       |
CPython arguments, buffer ownership,           |
output allocation, interpreter attachment      |
      |                                       |
      +-------------------+-------------------+
                          v
             Algorithm and input-size dispatch
                          |
                          v
                 SIMD or scalar kernel
                          |
                          v
                Tail handling and result
```

SIMD (single instruction, multiple data) kernels process several bytes or words per instruction. The Python
layer adds argument and ownership checks around the same Rust algorithms. Kernels have no dependency on Python.

| Source | Responsibility |
| --- | --- |
| [`src/backend.rs`][cpu] | Detect and cache CPU capabilities. |
| [`src/base64/`][base64] | Encode and decode flows, alphabets, output sizing, and instruction-set kernels. |
| [`src/murmur3/`][murmur3] | Canonical variants, incremental state, block buffering, and dispatch. |
| [`src/xxhash/`][xxhash] | Length-specific formulas, long-input accumulation, prepared seeds, and batches. |
| [`src/bindings/`][bindings] | CPython arguments, buffers, compatibility rules, and native callbacks. |
| [`hashcodecs/`][python] | Generated Python exports, type stubs, and the `py.typed` marker. |

Each algorithm exposes its public Rust API through `src/<algorithm>.rs`. Internal modules own validation and
state; instruction-set modules own the vector operations. The feature-gated bindings share the crate with the
core so they can use private output-pointer APIs without an intermediate result copy.

Keep shared binding policy in `arguments.rs`, `buffer.rs`, `objects.rs`, and `runtime.rs`. Algorithm adapters
compose those policies; `bindings.rs` registers the extension. The checked-in `_hashcodecs.pyi` declaration drives
Python exports, stubs, native signatures, and API-reference lists through `tools/generate_api_metadata.py`.
Generated Python modules reexport native functions without a Python wrapper call.

## Runtime dispatch

[`backend.rs`][cpu] detects CPU features once through `OnceLock`. Each algorithm selects kernels from that cached
capability set and checks the features its kernels and fallback paths require.

| Operation | x86 and x86-64 preference | AArch64 |
| --- | --- | --- |
| Base64 | AVX-512 VBMI, AVX2, SSE4.1, SSSE3, scalar | NEON, scalar |
| MurmurHash3 | AVX2, SSE4.1, scalar, subject to variant and size thresholds | Scalar |
| XXH3 above 240 bytes | AVX-512F, AVX2, SSSE3, scalar | NEON, scalar |

Base64 caches its selected backend and cache policy. XXH3 caches its long-input engine. MurmurHash3 selects a
backend for each batch of complete blocks. Small inputs and tails use narrower kernels or scalar code to avoid
vector setup that exceeds the work. Unsupported architectures, Miri, and Kani use scalar implementations.

Runtime checks allow one build to run on Intel, AMD, and hosts without the required SIMD extensions. Backend
priority is a dispatch policy; input size, cache state, and CPU design still affect throughput.

## Base64: vector conversion and bounded stores

### Convert and validate blocks

Base64 maps three binary bytes to four six-bit alphabet indices. The encoders use vector shuffles, shifts, and
arithmetic to form several groups at once, then translate indices to ASCII. The AVX2 encoder produces 32 output
bytes per 24 input bytes; its x86-64 bulk loop groups 96 input bytes into 128 output bytes to share loop overhead.

The decoders combine character classification and translation in vectors. The SSE kernels use lookup tables
indexed by the high and low four bits of each character to detect invalid alphabet entries. They pack valid
six-bit indices into bytes. The SSSE3 and SSE4.1 loops validate 64 input characters per group; AVX2 validates 128.
Combining the error masks gives one validity decision per group.

```text
SSE decode group with exact output: 64 characters -> 48 bytes

Input blocks       [16 chars] [16 chars] [16 chars] [16 chars]
                        \        |          |        /
                         Combined alphabet validation
                                      |
Output ranges      Data bytes        Overlap with the next block
Store 1            [0, 12)            [12, 16)
Store 2            [12, 24)           [24, 28)
Store 3            [24, 36)           [36, 40)
Final stores       [36, 44), [44, 48) None
```

After validating all four blocks, the SSE decoder can overlap the first three 16-byte stores. The next store
overwrites the four extra bytes. For exact outputs, two final stores write the last 12 bytes within the boundary.
This uses five stores for 48 decoded bytes and preserves the unwritten suffix when a prefix decoder stops at
invalid input. AVX2 uses the same validated-group principle and selects a store layout from output alignment.

Scalar code handles the remaining groups and padding. Rust allocating APIs reserve uninitialized storage and
expose the result after initializing its returned prefix. `*_into` APIs validate capacity and write to the
caller's output. See the [encoding kernels][base64-encode], [decoding kernels][base64-decode], and
[`output_buffer.rs`][base64-output] for these contracts.

### Limit memory traffic

For large x86-64 encoding, the [cache policy][cache] estimates whether input plus output fits in private cache.
Encoding reads three bytes for each four bytes it writes, so the input limit is about `3/7` of detected cache
capacity. Above that limit, eligible aligned paths can use non-temporal stores to reduce cache pollution.
Smaller inputs, unknown cache topology, and unsuitable alignment keep cached stores.

The Python binding writes allocating results into CPython-owned memory. Reusable output avoids allocating a new
result object. For wrapped encoding above 4 MiB, a line-aware cursor accepts SIMD blocks and splits blocks at
newline boundaries, avoiding a separate full-output wrapping pass.

### Preserve Python decoding semantics

The [Base64 binding][base64-bindings] shares one attempt policy between allocating and reusable outputs. Clean
input can reach the SIMD core without constructing a Python exception. Lenient decoding scans alphabet runs and
handles ignored bytes and padding through the CPython-compatible state machine. Configured decoding uses a
4 KiB staging buffer for translated runs; complete untranslated groups can bypass that copy.

Direct prefix probes write complete validated blocks and keep the remaining suffix for a retry. Custom-alphabet
probes validate the input before decoding. Strict attempts can write within a failing block, so callers must not
assume transactional output on failure. Malformed cases that need CPython's exact exception fall back to
`binascii`. Compatibility policy includes interpreter-specific padding and argument-conversion order.

## MurmurHash3: parallel block preparation

MurmurHash3 combines independent per-block multiplication and rotation with an ordered hash-state update.
The SIMD kernels prepare several blocks together, then feed those values through the canonical state sequence.
For example, the x64-128 AVX2 loop prepares 128 bytes at a time in stack storage. AVX2 lacks a low-64-bit integer
multiply instruction, so the kernel composes it from 32-bit products. The resulting digest preserves the scalar
algorithm's operation order.

Vector preparation has a setup cost. The [dispatcher][murmur-dispatch] applies these thresholds to complete blocks:

| Variant | AVX2 minimum | SSE4.1 fallback range |
| --- | ---: | ---: |
| x86-32 | 32 B | At least 16 B |
| x86-128 | 256 B | At least 16 MiB |
| x64-128 | 512 B | 512 B through 8 MiB |

Below an eligible range, the dispatcher selects scalar code. Incremental updates apply the thresholds to each
batch of full blocks. `block_buffer.rs` keeps pending bytes and their length together, processes complete input
blocks without copying the whole update, and retains the incomplete tail for the next call.

## XXH3: independent accumulator chains

[`one_shot.rs`][xxhash-one-shot] selects formulas for 0–16, 17–128, and 129–240 bytes, with additional fixed-size XXH3-128
paths. Inputs above 240 bytes use 64-byte stripes and an eight-lane accumulator. The long-input flow processes
1,024-byte blocks, scrambles the accumulator between blocks, and merges it into a 64-bit or 128-bit digest.

The [AVX2 kernel][xxhash-avx2] distributes a block's 16 stripes across four accumulator chains. Each chain depends
on its own previous value, so the CPU can overlap arithmetic across chains instead of waiting on one long
dependency sequence. Stripe accumulation is additive, which permits a reduction before the required scramble.

```text
One 1,024-byte XXH3 block: 16 stripes of 64 bytes

Chain 0:  stripe 0  -> stripe 4 -> stripe 8  -> stripe 12 --+
Chain 1:  stripe 1  -> stripe 5 -> stripe 9  -> stripe 13 --+
Chain 2:  stripe 2  -> stripe 6 -> stripe 10 -> stripe 14 --+--> add chains
Chain 3:  stripe 3  -> stripe 7 -> stripe 11 -> stripe 15 --+       |
                                                                v
                                                        canonical scramble
```

The tail uses four chains when it contains at least three regular stripes plus the final overlapping stripe.
The kernel reduces those chains before the final merge. This schedules independent instructions within one
thread; it does not start worker threads.

Native batches reuse seed setup and inspect up to four adjacent inputs. Groups of two to four long inputs with
the same regular-stripe count use an AVX2 batch kernel when available. Each input keeps its own final stripe;
equal stripe counts permit different byte lengths. Other items use the single-input paths, preserving input order.

`PreparedXxh3` derives a nonzero seed's 192-byte secret once for repeated long-input calls. Short inputs use their
length-specific formulas. The Rust `*_batch_for_each` APIs deliver digests to a callback without allocating a
result vector. Python list batches allocate integers; packed `*_batch_into` calls write 8 or 16 little-endian
bytes per digest into one destination.

## CPython: avoid copies and per-item calls

The extension uses version-specific CPython APIs and `METH_FASTCALL | METH_KEYWORDS` callbacks. Native parsers
read arguments without a Python wrapper or an argument tuple on that call path. A batch call shares that entry
cost across its items. Buffer ownership then determines whether the binding can borrow data or needs a snapshot.

| Input or output | Handling and constraint |
| --- | --- |
| Exact `bytes` | Borrow immutable data without an input copy. |
| Contiguous memoryview of exact `bytes` | Borrow a small view while attached, or retain its immutable owner and slice offset. Full and sliced views can avoid copies, including on free-threaded builds. |
| Exact `bytearray` or a view over mutable storage | Borrow under interpreter or object synchronization; snapshot where callbacks, overlap, or free-threaded access require stable data. |
| Non-contiguous view | Hashing and Base64 decoding flatten the view; Base64 encoding requires C-contiguous input. |
| Exact ASCII `str` for standard Base64 decoding | Borrow the string's UTF-8 representation without an intermediate ASCII `bytes` object. String subclasses retain their `encode` behavior. |
| Base64 `*_into` output | Check capacity, stabilize overlapping input, and preserve bytes beyond the returned length. |

[`buffer.rs`][buffers] owns these rules. Arbitrary buffer exporters and Python callbacks can run user code, release
views, or resize mutable storage. The binding must stabilize affected inputs before retaining raw pointers across
those operations. A read-only view of mutable storage still needs the owner's synchronization policy.

### Interpreter detachment

On GIL-enabled CPython, detaching releases the global interpreter lock (GIL) so other Python threads can run.
The [runtime policy][runtime] keeps short calls attached to avoid release and reacquisition overhead. Eligible
immutable or snapshotted inputs use these thresholds:

| Workload | Detachment threshold |
| --- | --- |
| One-shot Base64 and XXH3 | 256 KiB of input |
| One-shot MurmurHash3 | 64 KiB of input |
| XXH3 batches | 1 MiB of total input or 16,384 items |

The binding does not borrow mutable input across detached regions. For detached packed XXH3 batches, it retains input
owners and stages digests, then reacquires synchronization and rechecks destination capacity before publishing
the output. The exact-`bytes` path borrows 64 retained inputs at a time in a stack array, avoiding a full-batch
array of slice descriptors. Small stable batches can write packed digests without staging results.

The item threshold accounts for per-item work even with empty inputs. These thresholds trade single-call latency
against interpreter availability; they do not bound how long another thread waits. See the
[XXH3 batch binding][xxhash-batches] for detachment and output publication.

## Allocation and build policy

Release and benchmark profiles use optimization level 3, one code-generation unit, and full link-time
optimization. The Python extension and Rust benchmarks select `mimalloc` for Rust allocations; CPython owns its
Python object allocations. Rust crate consumers retain their allocator choice.

Production codec and hash implementations live in this repository. Competitor crates serve tests and benchmarks
through development dependencies. The Python wheel build uses Hatchling and a Rust build with the
`extension-module` feature.

## Correctness constraints

Kernel changes must preserve canonical outputs, supported CPU checks, and input and output bounds. Validation
covers malformed data, lengths around vector boundaries, available backends, and exact output slices.

- Rust and Python differential tests compare outputs with reference implementations. CPython tests also check
  version-specific errors, conversion order, aliasing, and interpreter progress.
- Allocating Base64 batches discard partial result lists on failure. Reusable Base64 batches retain prior
  destination writes. Packed XXH3 batches validate and stabilize inputs before mutating the destination.
- Miri, Kani, fuzzing, and sanitizer checks cover pointer and buffer invariants. See [SAFETY.md][safety] for scope.
- Core coverage runs without default features and requires 100% line coverage, with the hardware-only AVX-512
  filename exclusion. Python facade branch coverage requires 100%; the Python suite behavior-tests the bindings.

Measure kernel and API costs separately when assessing an optimization. Rust benchmarks exercise the core;
Python benchmarks include parsing, ownership, object allocation, and detachment. Compare allocating and reusable
outputs with the corresponding [benchmark workloads][benchmarks].

[benchmarks]: https://github.com/kozistr/hashcodecs-rs/blob/main/BENCHMARK.md
[cpu]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/backend.rs
[base64]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/base64
[base64-encode]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/base64/encode
[base64-decode]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/base64/decode
[base64-output]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/base64/output_buffer.rs
[cache]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/base64/encode/cache.rs
[base64-bindings]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/bindings/base64
[murmur3]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/murmur3
[murmur-dispatch]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/murmur3/dispatch.rs
[xxhash]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/xxhash
[xxhash-one-shot]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/xxhash/one_shot.rs
[xxhash-avx2]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/xxhash/long_inputs/x86/avx2.rs
[bindings]: https://github.com/kozistr/hashcodecs-rs/tree/main/src/bindings
[buffers]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/bindings/buffer.rs
[runtime]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/bindings/runtime.rs
[xxhash-batches]: https://github.com/kozistr/hashcodecs-rs/blob/main/src/bindings/xxhash/batch.rs
[python]: https://github.com/kozistr/hashcodecs-rs/tree/main/hashcodecs
[safety]: https://github.com/kozistr/hashcodecs-rs/blob/main/SAFETY.md
