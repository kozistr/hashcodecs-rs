# Performance Log

This is the decision record for performance-sensitive changes. It follows the useful pattern in
[gigatoken's optimization log](https://github.com/marcelroed/gigatoken/blob/main/pretokenizer_optimization_log.md):
record the baseline, the attempted change, the measurement, and whether the change stayed.

## Measurement Rules

- Validate output against the reference implementation before timing it.
- Pin one logical CPU, disable Python garbage collection during samples, and use clean release builds.
- Measure the affected operation across boundary sizes and representative large inputs. Do not optimize only the largest case.
- Compare identical ISAs. The Windows xxHash C baseline is built with AVX2 because that is the backend selected on the benchmark host.
- Keep an optimization only when repeated measurements show a useful improvement without a correctness, portability, or API regression.
- Refresh only the charts and CSV rows affected by a measured change. Record the CPU, OS, command, and sample count.

The reproducible commands and benchmark host are documented in [BENCHMARK.md](../BENCHMARK.md) and the
[README](../README.md#run-locally).

## Current Policies

| Area | Policy | Reason |
| --- | --- | --- |
| CPU dispatch | Cache one process-wide capability snapshot and select the best supported backend. | Avoid repeated feature detection while keeping unsupported instructions unreachable. |
| Base64 | Prefer AVX-512 VBMI, AVX2, SSE4.1, SSSE3, NEON, then scalar. | Preserve broad portability while selecting the widest proven kernel. |
| Base64 stores | Use non-temporal x86 stores only when the input exceeds 3/7 of detected private L1/L2 capacity and the output is 16-byte aligned. | Avoid polluting private caches for memory-bound output without penalizing smaller inputs. |
| MurmurHash3 | Dispatch by both ISA and measured length thresholds in `src/murmur3/dispatch.rs`. | SIMD setup is not free and scalar wins below some boundaries. |
| XXH3 | Use canonical 0-16, 17-128, 129-240, and long-input paths; SIMD accumulation is reserved for long inputs. | Match the reference algorithm's natural size classes. |
| Python GIL | Detach immutable work at 64 KiB or larger; retain the GIL for mutable buffers. | Amortize detach overhead and prevent concurrent mutation of borrowed storage. |
| Python memoryview | Copy small or sliced views; reuse a full contiguous bytes/bytearray owner from 4 KiB upward. | Avoid expensive owner introspection for small views while eliminating large copies. |
| Python output | Prefer caller-managed `bytearray` APIs where repeated allocation or Python object creation matters. | Makes allocation explicit and permits stable reusable storage. |

## Decision Log

| Date | Change | Measurement and decision |
| --- | --- | --- |
| 2026-08-22 | Direct CPython vectorcall callbacks for native functions (`#31`). | Kept. Removes wrapper and generic argument-parsing overhead; the affected Python charts were refreshed with correctness gates intact. |
| 2026-08-22 | Version-specific CPython APIs instead of `abi3` (`#33`). | Kept. This targets overhead-bound calls, matching gigatoken's observation that stable-ABI iteration carries a material cost. Wheels remain built per supported CPython version. |
| 2026-08-22 | `xxh3_64_batch_into` and `xxh3_128_batch_into`. | Kept. Reusing packed output was 2.49x/2.67x faster at 2 KiB, 1.43x/1.89x at 32 KiB, and effectively even at 32 MiB for 64/128-bit batches. This removes Python integer allocation where it matters without slowing throughput-bound large batches. |

### XXH3 reusable output baseline

Host: Windows 10 x64, Intel Core Ultra 7 265K, CPython 3.12.10. Command:

```sh
uv run --no-project --with . python benchmarks/python_xxhash.py --hashcodecs-only
```

| Total input | XXH3-64 list | XXH3-64 into | XXH3-128 list | XXH3-128 into |
| ---: | ---: | ---: | ---: | ---: |
| 2 KiB | 5.02 GiB/s | 12.50 GiB/s | 2.68 GiB/s | 7.15 GiB/s |
| 32 KiB | 43.31 GiB/s | 62.02 GiB/s | 28.67 GiB/s | 54.24 GiB/s |
| 128 KiB | 61.92 GiB/s | 71.46 GiB/s | 53.49 GiB/s | 68.72 GiB/s |
| 32 MiB | 36.84 GiB/s | 36.68 GiB/s | 36.69 GiB/s | 36.61 GiB/s |

The reusable buffers were allocated once outside the timed operation. Both paths hash the same 32 equal-size inputs, and the benchmark validates the packed little-endian output before sampling.

## Change Template

Add one row above with the date and summary, then include these details when the result needs more context:

```text
Host / toolchain:
Command:
Baseline:
Candidate:
Correctness checks:
Decision:
```

A rejected experiment belongs here too. Knowing what lost, and at which sizes, prevents the same attractive mistake from returning later.
