# ADR 0002: Feature-First Algorithm Modules

- Status: Accepted
- Date: 2026-08-23

## Context

The crate once kept most MurmurHash3 and XXH3 scalar logic in `src/murmur3.rs` and `src/xxhash.rs`. Base64 used
operation modules under `src/base64/`. The root files mixed public API documentation with algorithm bodies,
incremental state, batching, safety proofs, and tests.

A technical-layer layout would group all public APIs, scalar functions, dispatchers, and SIMD kernels across the
crate. A fixed per-algorithm template would give Base64, MurmurHash3, and XXH3 the same filenames. Both options
scatter related changes or force unrelated algorithms into the same shape.

## Decision

Keep one feature slice for each algorithm. Each slice exposes a small root façade and stores its implementation in
a sibling directory.

- Base64 splits by operation: encode and decode. Each operation owns flat architecture-kernel modules. Keep an
  architecture-family module only when multiple kernels share contracts.
- MurmurHash3 splits by canonical variant: x86-32, x86-128, and x64-128.
- XXH3 splits by processing stage. The `long` module owns its scalar flow and flat ISA kernel modules.
- Shared primitive modules contain leaf functions and constants with no public API or binding dependencies.
- Architecture kernels depend on primitive and long-pipeline modules through crate-private interfaces.
- Test, Miri, and Kani modules sit beside the functionality under test.
- Python adapters follow the same feature slices and separate callbacks from registration metadata when both grow.

Keep direct function calls, existing inline attributes, and runtime CPU dispatch. Module boundaries add no dynamic
dispatch to hashing or codec paths.

## Consequences

A change to one MurmurHash3 variant stays in one variant module. XXH3-64 and XXH3-128 share the implementation
under `xxhash/long/`. Base64 keeps separate encode and decode flows without adding an extra directory for an
architecture family.

Developers must learn three internal shapes. The repository documents each shape in `docs/ARCHITECTURE.md`, and
the shared dependency rules from ADR 0001 still apply to all three.
