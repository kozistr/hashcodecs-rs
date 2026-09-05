# Unsafe-code verification

Base64, MurmurHash3, and XXH3 limit raw pointers to tightly scoped word loads,
exact-output stores, and architecture-specific SIMD kernels. Runtime feature
detection and length dispatch establish their contracts before they are reached.

The checks are deliberately complementary:

- Kani proves Base64 scalar output boundaries, Base64 decoder store widths,
  MurmurHash3 unaligned reads and scalar block loops, plus XXH3 word loads and
  long-input SIMD scheduling bounds.
- Miri interprets Base64 allocation and exact-buffer boundaries, MurmurHash3
  one-shot and incremental boundaries, and every XXH3 length class with strict
  provenance. Hardware intrinsics are disabled because Miri cannot interpret
  them.
- AddressSanitizer and MemorySanitizer exercise the runtime-selected SIMD paths,
  exact Base64 buffers, incremental MurmurHash3 states, and XXH3 batch kernels.
- libFuzzer runs under sanitizers. Base64 is compared with `base64` 0.23.1,
  MurmurHash3 with `murmur3` 0.5, and XXH3 with `xxhash-rust` 0.8.18.
  XXH3 cases use independent equal-length and heterogeneous buffers, with
  distinct lane contents and batch counts through nine to cover SIMD groups and tails.

The Python XXH3 batch bindings retain immutable owners during result allocation.
For mutable buffers, they finish hashing before allocating Python result containers:
a GC finalizer can resize a bytearray even while the GIL is held. Subprocess tests
on CPython 3.10/3.11 trigger finalizers during allocation to check both invariants.

Run the same checks locally on Linux:

```sh
cargo kani
MIRIFLAGS=-Zmiri-strict-provenance \
  cargo +nightly miri test --lib miri_tests
RUSTFLAGS="-Zsanitizer=address" \
RUSTDOCFLAGS="-Zsanitizer=address" \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu \
  --test sanitizers
RUSTFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
RUSTDOCFLAGS="-Zsanitizer=memory -Zsanitizer-memory-track-origins" \
  cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu \
  --test sanitizers
cargo +nightly fuzz run base64 -- -max_total_time=30 -rss_limit_mb=2048
cargo +nightly fuzz run murmur3 -- -max_total_time=30 -rss_limit_mb=2048
cargo +nightly fuzz run xxhash -- -max_total_time=30 -rss_limit_mb=2048
```
