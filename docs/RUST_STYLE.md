# Rust style

Use the [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html) across Rust implementations,
bindings, tests, benchmarks, fuzz targets, and generated-code templates, with the adaptations below. The
[repository instructions](https://github.com/kozistr/hashcodecs-rs/blob/main/AGENTS.md) take precedence. Changes
to the upstream guide require review before they change this policy. Apply style improvements within the scope
of a change; keep broad migrations separate.

## Formatting and review

Stable rustfmt defines formatting, with the checked-in 2024 style edition and 100-column target. Keep imports
grouped by their immediate module and scope traits locally when practical. Review these import rules manually;
formatter options unavailable on the minimum supported stable toolchain are not required.
Use lowercase hex for new constants. Reserve `#[rustfmt::skip]` for generated code or meaningful table alignment.

Import types directly and qualify free functions by module when that makes their origin clearer. Explicitly
named algorithm helpers may retain direct imports. Allow glob imports in architecture intrinsic modules,
test modules using `super::*`, non-leaf facade re-exports, and the PyO3 prelude. Use conventional algorithm and
instruction names such as XXH3, AVX2, SSSE3, and NEON; expanding those names would obscure their meaning.

Prefer matching over checking and unwrapping. Explain invariant panics with `expect` and useful assertion
messages. Test assertions may use `unwrap` when the surrounding case already explains the expected success.
Keep loops and iterator chains readable. Document public behavior, errors, panic conditions, and unsafe
preconditions. Existing `missing_docs` and Clippy checks remain in force.

The root crate and benchmark crate inherit the Cargo workspace lint policy; the isolated fuzz workspace mirrors
it. `just format`, `just lint`, and CI check the fuzz targets too. `unsafe_op_in_unsafe_fn` requires
explicit unsafe blocks, and `unused_must_use` rejects discarded results that require handling. Clippy's
`implicit_clone` exposes copies hidden behind conversion methods; `clone_on_ref_ptr` distinguishes shared
ownership clones from deep copies. The policy also checks public result annotations,
error and panic documentation, and wildcard imports. Intrinsic modules scope the documented import exception
to each architecture import. `just lint` promotes Clippy warnings to errors. Other style rules remain review
responsibilities; broad restriction and pedantic lint groups are not enabled.

## Local design choices

| Area | Repository rule and reason |
| --- | --- |
| Crate and file layout | Keep the Cargo module tree and feature-gated Python bindings. Chromium's flat files, tiny crates, import mechanism, threading rules, and Fuchsia's mutex choice do not apply. Non-leaf modules should primarily organize and re-export code; small dispatch or binding glue may stay alongside submodules. |
| Input and output types | Keep contiguous `&[u8]`, exact `&mut [u8]` destinations, and existing allocating APIs. SIMD loads and length checks need slices. Batch callbacks already let callers avoid allocating results. Iterator or `Into` generalization needs a concrete caller benefit and performance review. |
| Ownership | Prefer borrowing and reusable storage. Use `Rc` or `Arc` when shared lifetimes are needed. Keep copies that stabilize mutable or overlapping Python buffers across callbacks or interpreter detachment. |
| Dispatch and abstraction | Keep const generics, internal Boolean mode flags, function pointers, inline annotations, and CPU feature gates where kernels need them. Prefer enums for unclear flags in new public APIs. Evaluate generics and `dyn Trait` using the actual call path and code size. |
| Public types | Choose `#[non_exhaustive]` according to each type's evolution contract. `Base64Error` already permits extension. Keep closed internal backend enums exhaustive. Changes to existing public matching or construction require compatibility review. Preserve documented hash word order and result types. |
| Traits and attributes | Derive useful standard traits and provide `Default` for a meaningful default state. Add `#[must_use]` when ignoring a result is a mistake; a `Result` already carries it. New `const` APIs need a lasting compatibility commitment. Avoid redundant constructor wrappers and speculative abstractions. |
| Macros and generation | Keep binding schema generation and SIMD macros when they prevent duplicated contracts. Edit their source and regenerate outputs; a style migration must preserve generated interfaces. |
| Panic boundaries | Keep CPython panic containment at FFI entry points as defense in depth. Handle ordinary failures explicitly; unwinding must not cross the C ABI, and catching a panic does not restore invalid state. |

Unsafe kernels are permitted when required for intrinsics, FFI, output initialization, or a measured optimization.
Keep their safe entry points and the
[memory contracts](https://github.com/kozistr/hashcodecs-rs/blob/main/SAFETY.md). For new or changed unsafe code,
document interface preconditions and explain non-obvious bounds, initialization, ownership, or CPU feature
invariants where they are established. Describe a shared invariant once at the relevant module, trait, or
operation; use local `SAFETY:` comments when a particular operation needs additional explanation. Do not repeat
a function's contract before every intrinsic or forwarding call. Review explanations in context rather than
enabling `clippy::undocumented_unsafe_blocks`, which requires comments on individual blocks even when they share
one proof. State why safe code is insufficient and provide benchmark evidence when performance is the reason.
Safety requirements apply regardless of benchmark results.

## Performance and validation

Formatting, documentation, and lint configuration do not change runtime operations. Any style refactor that
changes executable code needs an assessment of allocations, copies, bounds checks, dispatch, inlining, and code
size. When performance can change, compare the affected benchmark groups against the starting revision on the
same host with the same toolchain, flags, and one logical CPU. Follow the repository's timing rules and check
correctness outside timing. Resolve a repeatable regression before accepting the refactor; a passing test suite
alone does not establish performance.

Run `just format` and `just lint` before committing and `just full-check` before a PR. Preserve the existing
coverage requirements and exercise relevant available SIMD backends when implementation changes. Benchmark
results describe the tested host; dispatch changes need evidence for the affected architectures. See
[Architecture](ARCHITECTURE.md) and [Performance](performance.md) for the existing mechanisms and measurements.

The [Hacker News discussion](https://news.ycombinator.com/item?id=50186010) provides useful review context,
particularly the tradeoffs around exhaustive matching, shared ownership, and measuring performance. Its
comments are not additional repository requirements.

## Source adoption

The existing source follows this policy as well as future changes. Base64 APIs, MurmurHash3 hasher implementations,
and the XXH3 long-input engine live in leaf modules; their original module paths re-export the public and shared
interfaces. Runtime selection and CPython registration retain small coordinating entry points.

The source audit covers import organization, type and lifetime names, constructor names, result handling,
ownership, standard traits, documentation, unsafe contracts, macros, and conditional architecture code.
Short-lived count pairs and intrinsic register tuples remain local and are destructured at use sites. Exported
hash word arrays retain their documented order. Existing slice and allocating APIs, closed backend enums, and
FFI panic containment follow the design choices above.
