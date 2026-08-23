# ADR 0001: Layered Module Boundaries

- Status: Accepted
- Date: 2026-08-23

## Context

`hashcodecs` combines portable algorithms, architecture-specific unsafe kernels, runtime CPU dispatch, and a
CPython extension. These concerns have different safety and performance constraints. Keeping their policies in
one module makes changes harder to review and lets language-binding details leak into algorithm code.

The incremental MurmurHash3 implementations also repeated the same pending-block state as separate buffer and
length fields. That allowed an invalid state to be represented even though the update paths kept the fields in
sync.

## Decision

Use one-way module dependencies:

```text
Python composition -> algorithm adapters -> binding policies -> CPython API
                                      |
                                      v
Rust public API -> dispatch policy -> scalar and SIMD kernels
```

- Keep `bindings/mod.rs` as the extension composition root.
- Put argument parsing, raw object access, buffer ownership, and execution policy in separate shared modules.
- Keep algorithm-specific callbacks within their algorithm adapter.
- Keep CPU feature detection shared and make dispatchers select from that capability snapshot.
- Encapsulate reusable state invariants, such as an incremental pending block, in a dedicated type.
- Preserve direct calls on hot paths; do not introduce dynamic dispatch solely to enforce these boundaries.

## Consequences

Unsafe CPython operations and GIL policy have clear review locations. Algorithm modules remain usable without the
`python` feature, and adapters can reuse policy without depending on each other. The additional modules add a
small amount of navigation, but each file has a narrower reason to change.
