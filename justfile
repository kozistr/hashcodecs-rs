set windows-shell := ['powershell.exe', '-NoLogo', '-Command']

docs_requirements := 'docs/requirements.txt'

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
    cargo test --features python
    uv run --frozen --no-sync pytest tests --cov=hashcodecs --cov-branch --cov-fail-under=100

coverage:
    cargo llvm-cov --no-default-features --fail-under-lines 100 --ignore-filename-regex 'avx512\.rs$' --show-missing-lines

test-release:
    cargo test --release --features python

# Run the fast local quality gates, including a strict documentation build.
check: format-check lint test docs-build

# Run every pre-commit gate from AGENT.md.
full-check: check test-release coverage

build:
    uv build

bench-base64:
    cargo bench --bench base64

bench-murmur3:
    cargo bench --bench murmur3

bench-xxhash:
    cargo bench --bench xxhash
