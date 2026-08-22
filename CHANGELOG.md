# Changelog

This file records notable user-facing changes to `hashcodecs`. Version 1.0.0 starts the stable Python API policy.

## [Unreleased]

## [1.0.0] - Unreleased

### Added

- CPython 3.14t and 3.15t wheel support on Linux, macOS, and Windows.
- Complete API references for the Python and Rust interfaces.
- A compatibility policy for the stable Python API.

### Changed

- Base64 batch aliases now resolve through the native extension without Python wrappers.
- Runtime support and benchmark documentation now match the release artifacts.

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

[Unreleased]: https://github.com/kozistr/hashcodecs-rs/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.6.1...v1.0.0
[0.6.1]: https://github.com/kozistr/hashcodecs-rs/compare/v0.6.0...v0.6.1
[0.6.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/kozistr/hashcodecs-rs/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/kozistr/hashcodecs-rs/tree/v0.1.0
