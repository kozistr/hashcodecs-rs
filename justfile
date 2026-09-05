set windows-shell := ['powershell.exe', '-NoLogo', '-Command']

docs_requirements := 'docs/requirements.txt'

# Common optimization flags for ultra-minimal test output
cargo_flags := '-- --format=terse --quiet'
pytest_base := 'uv run --frozen --no-sync pytest'
pytest_flags := '-q --tb=no --disable-warnings --show-capture=no'

default:
    @just --list

# Start the local documentation server at http://127.0.0.1:8000.
docs:
    uv run --no-project --with-requirements {{ docs_requirements }} zensical serve

# Build the Read the Docs site without serving it.
docs-build:
    uv run --no-project --with-requirements {{ docs_requirements }} zensical build --strict

format:
    cargo fmt
    uv run --frozen --no-sync ruff check --fix .
    uv run --frozen --no-sync ruff format .

format-check:
    cargo fmt --check
    uv run --frozen --no-sync ruff format --check .

lint:
    cargo clippy -- -D warnings
    cargo clippy --all-targets --features python -- -D warnings
    uv run --frozen --no-sync ruff check .

test:
    cargo test --features python {{ cargo_flags }}
    uv run --frozen --no-sync python tools/install_local_wheel.py
    {{ pytest_base }} tests --cov=hashcodecs --cov-branch --cov-fail-under=100 {{ pytest_flags }} --cov-report=term:skip-covered

test-build:
    {{ pytest_base }} build_tests {{ pytest_flags }}

coverage:
    cargo llvm-cov --no-default-features --fail-under-lines 100 --ignore-filename-regex 'avx512\.rs$' --show-missing-lines {{ cargo_flags }}

test-release:
    cargo test --release --features python {{ cargo_flags }}

# Build and verify a wheel from an extracted source distribution.
verify-sdist:
    uv run --frozen --no-sync python tools/verify_sdist.py

# Run the fast local quality gates, including build-tool tests and a strict documentation build.
check: format-check lint test test-build docs-build

# Run every pre-commit gate from AGENTS.md, including extracted-sdist verification.
full-check: check test-release coverage verify-sdist

build:
    uv build

bench-base64:
    cargo bench --manifest-path benches/Cargo.toml --bench base64

bench-murmur3:
    cargo bench --manifest-path benches/Cargo.toml --bench murmur3

bench-xxhash:
    cargo bench --manifest-path benches/Cargo.toml --bench xxhash
