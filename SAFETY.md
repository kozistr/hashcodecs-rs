# Memory safety verification

Use this reference when reviewing changes to unsafe Rust or CPython buffer handling. It identifies the memory
contracts, the checks that exercise them, and the commands to reproduce those checks. For performance mechanisms,
see [Architecture](docs/ARCHITECTURE.md).

## Required invariants

| Boundary | Required contract | Source |
| --- | --- | --- |
| CPU dispatch | Check the features required by a kernel and its fallback paths before executing SIMD instructions. | [CPU detection](src/backend.rs), [Base64 dispatch](src/base64/backend.rs), [MurmurHash3 dispatch](src/murmur3/dispatch.rs), [XXH3 engine](src/xxhash/long_inputs.rs) |
| Input loads | Keep scalar word loads, vector loads, and final overlapping stripes within the input allocation. | [MurmurHash3 loads](src/murmur3/primitives.rs), [XXH3 loads](src/xxhash/primitives.rs), [XXH3 schedule](src/xxhash/long_inputs.rs) |
| Output stores | Check capacity before writing. Overlapping vector stores need space for their full width. Exact outputs must preserve bytes beyond the returned length. | [Base64 store policies](src/base64/decode/x86_contracts.rs), [output initialization](src/base64/output_buffer.rs) |
| Result initialization | Initialize the returned prefix before exposing uninitialized storage as a result. | [Base64 allocation](src/base64/output_buffer.rs), [XXH3 batch results](src/bindings/xxhash/batch.rs) |
| Python ownership | Keep owners alive and stabilize inputs before callbacks, overlapping writes, or interpreter detachment can invalidate a borrow. | [Buffer policy](src/bindings/buffer.rs), [Base64 batches](src/bindings/base64/batch.rs) |

Exact output bounds do not imply rollback on errors. Reusable Base64 batches retain prior destination writes.
Packed XXH3 batches validate and stabilize inputs before mutating the destination.

## CPython ownership and callbacks

The global interpreter lock (GIL) does not prevent callbacks from running on the same thread. Python allocation
can trigger garbage collection and finalizers that clear an input list or resize a `bytearray`. Argument
conversion and buffer release can also invoke user code. Bindings must preserve input ownership across these
operations and copy mutable or overlapping data where the buffer policy requires it.

XXH3 list batches finish input reads before allocating Python result containers. They store up to 32 native
results on the stack and use a vector with fallible allocation for larger batches. Base64 batches retain input
owners across result allocation. Subprocess tests on CPython 3.10 and 3.11 trigger finalizers during allocation
and reuse freed storage to check these lifetime rules.

Detached packed XXH3 batches retain immutable owners and stage digests in native memory. The path for exact
`bytes` borrows 64 retained inputs at a time in a stack array. After reattaching, the binding rechecks destination
capacity under synchronization before writing. The private `PackedDigest` contract requires initialized bytes
without padding and a representation matching the packed output on hosts that use little endian order. Those
hosts can copy the staged results in one operation. Other hosts serialize each word.

Tests cover source list mutation, output resizing, overlapping buffers, and access to mutable data on CPython
builds without the GIL. See the [XXH3 binding tests](src/bindings/xxhash/batch.rs),
[XXH3 Python tests](tests/test_xxhash.py), [Base64 batch tests](tests/test_base64_batch.py), and
[buffer tests](tests/test_base64_buffers.py).

## Verification scope

| Check | What it verifies | Limits |
| --- | --- | --- |
| Kani | Base64 scalar output bounds, MurmurHash3 and XXH3 word loads, MurmurHash3 scalar block loops, and XXH3 scheduling bounds. | Proofs apply within each harness's assumptions and unwind limits. The XXH3 schedule proof covers 241 through 3,072 bytes. |
| Miri | Scalar allocation, pointer provenance, exact Base64 outputs, MurmurHash3 incremental state, and XXH3 length classes and batches. | The Miri configuration uses scalar dispatch and does not execute hardware intrinsics. |
| AddressSanitizer | Invalid memory access in the sanitizer executable, including runtime SIMD paths, exact Base64 outputs, and hash batches. | Exercises inputs in the harness and backends available on the host. |
| MemorySanitizer | Reads of uninitialized memory in the same executable. | Exercises the instrumented Rust core, including a rebuilt standard library. |
| libFuzzer | Differential output checks against `base64`, `murmur3`, and `xxhash-rust` under sanitizers. | A timed run samples inputs. XXH3 batches cover one through nine independent buffers with equal or mixed lengths. |
| Python and Rust binding tests | Callback order, retained owners, alias snapshots, output publication, and thread coordination. | Some cases require a specific CPython version or a build without the GIL. |

The Rust Kani, Miri, sanitizer, and fuzz jobs exclude the optional CPython bindings. Binding tests cover that
boundary. Passing a check establishes its stated coverage, not a proof of all unsafe code.

Proof harnesses live in [Base64](src/base64/proofs.rs), [MurmurHash3](src/murmur3/proofs.rs), and
[XXH3](src/xxhash/proofs.rs). Miri cases live in the corresponding `miri_tests.rs` files.
The [sanitizer executable](tests/sanitizers.rs), [fuzz targets](fuzz/fuzz_targets), and
[CI workflow](.github/workflows/ci.yml) define the executed checks.
[Fuzz dependencies](fuzz/Cargo.toml) record the reference implementations.

## Run the Rust checks on Linux

Run these commands from the repository root on Linux x86_64 with a C/C++ compiler and linker installed.
Install nightly Rust with the `rust-src` and `miri` components, plus `cargo-fuzz`. Install
[Kani](https://model-checking.github.io/kani/install-guide.html) and complete `cargo kani setup` before running proofs.

```sh
rustup toolchain install nightly --component rust-src --component miri
cargo install cargo-fuzz --locked
```

Run the proofs and scalar interpreter checks:

```sh
cargo kani
MIRIFLAGS=-Zmiri-strict-provenance \
  cargo +nightly miri test --lib miri_tests
```

Run the sanitizer executable with each instrumentation mode:

```sh
RUSTFLAGS="-Zsanitizer=address" \
RUSTDOCFLAGS="-Zsanitizer=address" \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu \
  --test sanitizers

RUSTFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
RUSTDOCFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu \
  --test sanitizers
```

Run the bounded fuzz checks used in CI:

```sh
cargo +nightly fuzz run base64 -- -max_total_time=30 -rss_limit_mb=2048
cargo +nightly fuzz run murmur3 -- -max_total_time=30 -rss_limit_mb=2048
cargo +nightly fuzz run xxhash -- -max_total_time=30 -rss_limit_mb=2048
```

Successful runs exit with status zero and no failed proof, assertion, or sanitizer diagnostic. Preserve any
failing input or counterexample and reproduce the failure before changing the affected code.

## Run the CPython checks

Build the current wheel before testing the installed package:

```sh
uv sync --python 3.12 --frozen --no-install-project
uv run --python 3.12 --frozen --no-sync python tools/install_local_wheel.py
uv run --python 3.12 --frozen --no-sync pytest tests
uv run --python 3.12 --frozen --no-sync cargo test --features python
```

Repeat the setup and test commands with `3.10` or `3.11` for the finalizer subprocess cases and with `3.15t` for
cases that require CPython without the GIL. Those tests skip on incompatible interpreters. Check the selected
interpreter and skipped cases before treating a run as coverage of those behaviors.
