# Compatibility

## Stable Python API

The 1.x series treats the documented Python functions and classes exported through `__all__` in `hashcodecs`,
`hashcodecs.base64`, `hashcodecs.murmur3`, and `hashcodecs.xxhash` as public API. Names that start with an underscore
remain private.

Minor releases may add functions and keyword arguments. Patch releases may fix behavior that conflicts with the API
reference or CPython compatibility claims. A future removal or incompatible signature change will first raise a
deprecation warning for at least one minor release. Other breaking changes require a new major version.

## Python and platform support

`hashcodecs` supports CPython 3.10 through 3.15, including free-threaded CPython 3.14t and 3.15t. The release workflow
builds and tests these wheels:

| Operating system | Architecture | Wheel target |
| --- | --- | --- |
| Linux | x86-64 | manylinux 2.28 |
| macOS 11 or newer | ARM64 | macOS 11 ARM64 |
| Windows | x86-64 | Win32 AMD64 |

Users on other targets may build the source distribution with Rust 1.94 or newer. The maintainers do not claim wheel
support for targets outside this table.

## Rust API

The crates.io crate and PyPI package share one release version. The Rust API follows Cargo's Semantic Versioning
conventions. Before 1.0, minor releases may contain breaking Rust API changes; changes that affect Rust users are
documented in the changelog.
