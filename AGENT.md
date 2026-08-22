# Repository Guidelines

## Scope

`hashcodecs` provides optimized Base64 and MurmurHash3 implementations for Rust and Python 3.10+.

## Implementation

- Keep the codec and hash implementations in this repository. Do not replace production code with third-party codec crates.
- Preserve runtime CPU dispatch: AVX-512 VBMI, AVX2, SSE4.1, SSSE3, then scalar on x86/x86_64; NEON then scalar on AArch64.
- Keep behavior portable across Intel and AMD CPUs and hosts without SIMD support.
- Use `mimalloc` as the Python extension's global allocator without imposing it on Rust crate consumers.
- Keep benchmark competitors in development dependencies only.
- Prefer existing buffer APIs and avoid allocations or input copies on performance-sensitive paths.

## Python

- Use `uv` for environments, locking, builds, and test commands.
- Use Hatchling for packaging and build CPython wheels.
- Support Python 3.10 and newer, including interpreter-specific standard-library behavior.
- Configure Ruff for Python 3.12 with a line length of 119 and keep both lint and format checks clean.
- Keep type information through the checked-in stubs and `py.typed` marker.

## Validation

Run these gates before committing:

```sh
cargo fmt
cargo clippy -- -D warnings
cargo clippy --all-targets --features python -- -D warnings
cargo test --features python
cargo llvm-cov --no-default-features --fail-under-lines 100 --ignore-filename-regex 'avx512\.rs$' --show-missing-lines
uv run --frozen --no-sync ruff check .
uv run --frozen --no-sync ruff format --check .
uv run --frozen --no-sync pytest tests --cov=hashcodecs --cov-branch --cov-fail-under=100
```

Maintain 100% Rust line coverage and 100% Python branch coverage. The hardware-only AVX-512 implementation is the sole permitted filename exclusion; do not add other coverage exclusions or coverage-specific production branches. Cover malformed input, boundary lengths, every available SIMD backend, exact output-slice boundaries, and CPython differential cases.

## Benchmarks

- Keep Rust Base64, Rust MurmurHash3, and Python Base64 results grouped by functionality.
- Pin benchmark processes to one logical CPU and do not run benchmarks in CI.
- Benchmark only the functionality changed by the branch and refresh only its corresponding README or BENCHMARK charts.
- Use a complete clean benchmark run only for changes that can affect every benchmark group.

## Delivery

- Keep CI and release workflows separate.
- A release must pass CI before publishing to PyPI or crates.io and creating its GitHub release.
- GitHub releases must include generated changelog notes, wheels, and the source distribution.
- Publish the Rust crate and PyPI package with the same version from the release workflow.

## Commits

Every commit subject must start with one of these prefixes:

- `feat:`
- `fix:`
- `style:`
- `refactor:`
- `chore:`
- `build:`
- `update:`

Keep the text after the prefix imperative, concise, and specific to the committed change.

Note: do not use commit prefixes outside this list.
