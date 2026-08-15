# Unsafe-code verification

The XXH3 implementation limits raw pointers to two unaligned little-endian word
load helpers and architecture-specific SIMD kernels. Runtime feature detection
and length-class dispatch establish their contracts before they are reached.

The checks are deliberately complementary:

- Kani proves that arbitrary long-input schedules cannot overflow and that all
  64-byte data and secret loads remain in bounds.
- Miri interprets every XXH3 length class and batch fallback with strict
  provenance. Hardware intrinsics are disabled because Miri cannot interpret
  them.
- MemorySanitizer runs every x86 backend available on the runner plus the AVX2
  four-way batch kernel, detecting uninitialized reads in native SIMD code.
- libFuzzer runs under sanitizers and compares XXH3-64, XXH3-128, and their
  equal-size four-way batch APIs against `xxhash-rust` 0.8.18.

Run the same checks locally on Linux:

```sh
cargo kani
MIRIFLAGS=-Zmiri-strict-provenance \
  cargo +nightly miri test --lib miri_tests::every_length_class_and_batch_are_defined
RUSTFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
RUSTDOCFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu \
  --lib xxhash::tests::every_supported_x86_backend_matches_scalar -- --exact
cargo +nightly fuzz run xxhash -- -max_total_time=30 -rss_limit_mb=2048
```
