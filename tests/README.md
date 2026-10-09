# Test organization

Rust algorithm tests live under `src/{base64,murmur3,xxhash}/tests/`.
The corresponding `tests.rs` files declare the groups and shared fixtures.
Each group imports its own dependencies. Keep helpers local unless multiple groups use them.
Substantial private implementation suites use `#[cfg(test)] mod tests;` with
`<module>/tests.rs`; small helper tests remain inline beside their implementations.
Both forms are unit tests and can access private implementation details. Root-level
`tests/*.rs` files are integration tests of the public crate API.
Configured Base64 binding tests live under `src/bindings/base64/configured/tests/`.
Miri tests and `tests/sanitizers.rs` retain their separate entry points.

Python files use `test_<algorithm>_<behavior>.py`; the main hash test files cover
one-shot calls and public metadata.

| Group | Assertions |
| --- | --- |
| API and reference | Known digests, independent references, exports, signatures, and argument errors |
| Decoding and options | Padding, alphabets, canonical bits, ignored symbols, and interpreter defaults |
| Buffers and strings | Exact capacity, untouched suffixes, alias snapshots, buffer layouts, and ASCII conversion |
| Callbacks and fallback | Conversion order, reentrant hooks, warning cleanup, and CPython exception details |
| Batch | Ordering, mixed items, packed digests, capacity preflight, and partial failure |
| Incremental and prepared | Chunk boundaries, non-mutating digests, independent copies, and reusable seeds |
| Concurrency | GIL release, detachment boundaries, and free-threaded mutation races |

Keep Rust test names short and start with a verb, such as `reject_invalid_input`
or `preserve_output_bounds`; the module path supplies the algorithm and group.
Name Python tests `test_<behavior>`.
Parameterize independent inputs separately;
cross them when their interaction changes decoding, memory access, or callback order.
Keep exhaustive byte and lane classification in the Rust backend tests. Use representative
alphabet classes and lengths around block, staging, and detachment boundaries in Python.
Seed generated cases so failures can be reproduced. Keep interpreter-specific tests gated
by the running CPython version.

Run a focused group with `cargo test --lib murmur3::tests::incremental` or
`uv run --frozen --no-sync pytest tests/test_base64_strings.py`.
Run `just full-check` for all project gates, including 100% Rust core line coverage
and 100% Python facade branch coverage. Coverage alone does not establish memory safety
or CPython compatibility; retain boundary guards and differential assertions.
