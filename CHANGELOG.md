# Changelog

This file records notable user-facing changes to `hashcodecs`. Version 1.0.0 starts the stable Python API policy.

## [Unreleased]

## [1.3.0] - 2026-09-04

### What's Changed
* chore: expand XXH3 benchmark coverage by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/61
* refactor: split Base64 decoder bindings by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/62
* refactor: declare Base64 binding schema by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/63
* perf: inspect exact CPython memoryviews directly by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/64
* refactor: avoid copying Base64 fallback inputs by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/65
* chore: strengthen runtime coverage and benchmarks by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/66
* fix: avoid unnecessary batch snapshots and scalar grouping by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/67
* refactor: generate Base64 binding metadata by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/68
* perf: remove advanced decode and batch allocations by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/69
* refactor: finish native codec cleanup by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/70
* chore: verify source distributions in CI by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/71
* fix: stabilize aliased Base64 decode inputs by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/72
* perf: remove Python batch input copies by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/73
* fix: use strict fast paths for decode-into by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/74
* refactor: streamline native bindings and benchmarks by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/75
* fix: restore XXH fast paths and wheel compatibility by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/76
* refactor: use four-lane AArch64 XXH3 accumulation by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/77
* refactor: clarify internal names and technical prose by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/78
* fix: restore Base64 batch encode throughput by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/79
* refactor: clarify internal naming by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/80
* refactor: simplify internal decode and dispatch paths by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/81
* fix: harden hashers and optimize codec paths by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/82
* refactor: centralize Python Base64 decode routing by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/83
* refactor: standardize Rust callback and input names by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/84
* refactor: clarify codec routing and CPU capabilities by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/85
* refactor: consolidate base64 batch ownership by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/86
* refactor: reduce XXH3 dispatch overhead by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/87
* Fix Base64 batch alias stabilization and AVX2 streaming stores by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/88
* fix: restore Base64 encode fast paths by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/89
* fix: restore Base64 memoryview batch throughput by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/90


**Full Changelog**: https://github.com/kozistr/hashcodecs-rs/compare/v1.2.1...v1.3.0

## [1.2.1] - 2026-08-26

### What's Changed
* fix: harden buffer and lenient decode paths by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/58
* Refactor Murmur SIMD and centralize XXH3 long dispatch by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/59
* Repair sdist verification and roll back unpublished 1.2.1 by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/60


**Full Changelog**: https://github.com/kozistr/hashcodecs-rs/compare/v1.2.0...v1.2.1

## [1.2.0] - 2026-08-25

### What's Changed
* fix: harden free-threaded bindings and API correctness by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/47
* feat: accelerate Python batch outputs by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/48
* feat: add native lenient Base64 decoding by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/49
* update: tune Python detach thresholds by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/50
* fix: harden Base64 binding edge cases by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/51
* fix: stabilize Base64 GIL release test by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/52
* feat: accelerate XXH3 batch remainders by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/53
* perf: refresh Base64 batch benchmarks by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/54
* perf: accelerate XXH3 long inputs by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/55
* perf: profile Base64 batch allocations by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/56
* refactor: organize Python binding infrastructure by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/57


**Full Changelog**: https://github.com/kozistr/hashcodecs-rs/compare/v1.1.0...v1.2.0

## [1.1.0] - 2026-08-23

### What's Changed
* chore: use trusted publishing for crates.io by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/44
* refactor: establish clean architecture boundaries by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/45
* feat: borrow contiguous Python buffers by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/46


**Full Changelog**: https://github.com/kozistr/hashcodecs-rs/compare/v1.0.0...v1.1.0

## [1.0.0] - 2026-08-22

### What's Changed
* Expand Python and Rust API documentation by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/36
* feat: move base64 batch aliases into Rust by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/37
* feat: support free-threaded Python wheels by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/38
* Update benchmark and Python support docs by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/39
* chore: reduce CI runner usage and latency by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/40
* build: prepare 1.0 release by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/41
* build: publish Rust crate from release workflow by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/42
* feat: expose namespaced Rust API by @kozistr in https://github.com/kozistr/hashcodecs-rs/pull/43


**Full Changelog**: https://github.com/kozistr/hashcodecs-rs/compare/v0.6.1...v1.0.0

## [0.6.1] - 2026-08-22

### Added

- Reusable output buffers for XXH3 batch operations.
- A Read the Docs site with Python and Rust API documentation.

## [0.6.0] - 2026-08-21

### Added

- SIMD-accelerated XXH3-64 and XXH3-128 APIs.
- Native one-shot and batch bindings built on the full CPython API.

### Changed

- Faster Base64 and XXH3 hot paths, including small XXH3 inputs and large AVX2 Base64 inputs.
- One runtime dispatch layer now selects SIMD backends for each codec and hash implementation.

## [0.5.0] - 2026-08-12

### Added

- Batch Base64 APIs that write into caller-provided buffers.

### Changed

- Faster unpadded Base64 and Python decoding paths with fewer aligned-input copies.
- Release validation now runs before packaging and publication.

## [0.4.0] - 2026-08-10

### Added

- CPython 3.15 support.
- Native Base64 batch APIs.

### Changed

- Faster SIMD Base64 codecs and mutable Python buffer handling.
- Simpler URL-safe Base64 wrappers.

## [0.3.0] - 2026-08-09

### Added

- Reusable Python output buffers for allocation-sensitive Base64 operations.

### Changed

- Faster AVX2 Base64 decoding and SIMD encoding paths.

## [0.2.0] - 2026-08-09

### Added

- Incremental MurmurHash3 hashers.
- SIMD implementations for each MurmurHash3 variant and additional codec backends.

### Fixed

- Short NEON Base64 blocks and scalar backend detection on non-x86 systems.

## [0.1.0] - 2026-08-09

### Added

- Initial Python and Rust APIs for Base64 and MurmurHash3.
- Runtime SIMD dispatch and platform-specific CPython wheels.

[Unreleased]: https://github.com/kozistr/hashcodecs-rs/compare/v1.3.0...HEAD
[1.3.0]: https://github.com/kozistr/hashcodecs-rs/compare/v1.2.1...v1.3.0
[1.2.1]: https://github.com/kozistr/hashcodecs-rs/compare/v1.2.0...v1.2.1
[1.2.0]: https://github.com/kozistr/hashcodecs-rs/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/kozistr/hashcodecs-rs/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.6.1...v1.0.0
[0.6.1]: https://github.com/kozistr/hashcodecs-rs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kozistr/hashcodecs-rs/tree/v0.1.0
