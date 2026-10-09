# Test organization

Keep Rust algorithm tests under `src/{base64,murmur3,xxhash}/tests/`.
Use each `tests.rs` for module declarations and shared fixtures. Import dependencies
in the group that uses them, and keep helpers local unless multiple groups need them.

Put larger implementation suites in `<module>/tests.rs` with `#[cfg(test)] mod tests;`.
Keep small helper tests inline. Use either layout to test private code; put public API
integration tests in root `tests/*.rs` files. Keep configured Base64 binding tests in
`src/bindings/base64/configured/tests/`. Use `miri_tests.rs` for Miri suites and
`tests/sanitizers.rs` for sanitizer checks.

Start Rust test names with a short verb phrase: `reject_invalid_input` or
`preserve_output_bounds`. Use the module path for the algorithm and group.
Name Python files `test_<algorithm>_<behavior>.py` and functions `test_<behavior>`.
Keep one-shot hash calls and public metadata in the main hash test files.

| Group | Assertions |
| --- | --- |
| API and reference | Known digests, independent references, exports, signatures, and argument errors |
| Decoding and options | Padding, alphabets, canonical bits, ignored symbols, and interpreter defaults |
| Buffers and strings | Exact capacity, untouched suffixes, alias snapshots, buffer layouts, and ASCII conversion |
| Callbacks and fallback | Conversion order, reentrant hooks, warning cleanup, and CPython exception details |
| Batch | Ordering, mixed items, packed digests, capacity checks, and partial failure |
| Incremental and prepared | Chunk boundaries, digests that preserve state, independent copies, and reusable seeds |
| Concurrency | GIL release, detachment boundaries, and free-threaded mutation races |

Parameterize independent inputs; combine them when their interaction changes decoding,
memory access, or callback order. Check the full byte range at each SIMD lane in Rust.
Use representative alphabets and lengths around block, staging, and detachment boundaries
in Python. Fix random seeds and gate CPython-specific cases by interpreter version.

Run a Rust group or a Python file:

```sh
cargo test --lib murmur3::tests::incremental
uv run --frozen --no-sync pytest tests/test_base64_strings.py
```

Run `just full-check` before opening a PR. Maintain 100% Rust core line coverage and
100% Python facade branch coverage. Retain buffer guards and CPython differential checks.
