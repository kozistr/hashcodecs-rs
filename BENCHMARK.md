# Benchmark Details

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and 15 Python samples. The upstream C baseline was compiled explicitly for AVX2, matching the backend selected by hashcodecs on this host. Higher is better.

## XXH3

The Rust comparison calls xxHash 0.8.3 through `xxhash-c-sys`. Python compares with the upstream `xxhash` extension. Batch rows contain 32 equal-size inputs.

| API | Variant | Input | hashcodecs | upstream | Speedup |
| --- | --- | ---: | ---: | ---: | ---: |
| Rust one-shot | XXH3-64 | 64 B | **32.00 GiB/s** | 26.15 GiB/s | **1.22x** |
|  |  | 1 KiB | **25.65 GiB/s** | 24.20 GiB/s | **1.06x** |
|  |  | 4 KiB | **53.75 GiB/s** | 36.57 GiB/s | **1.47x** |
|  |  | 1 MiB | **73.91 GiB/s** | 45.94 GiB/s | **1.61x** |
|  |  | 8 MiB | **44.83 GiB/s** | 36.93 GiB/s | **1.21x** |
|  | XXH3-128 | 64 B | **9.26 GiB/s** | 7.43 GiB/s | **1.25x** |
|  |  | 1 KiB | **27.32 GiB/s** | 20.50 GiB/s | **1.33x** |
|  |  | 4 KiB | **49.41 GiB/s** | 35.34 GiB/s | **1.40x** |
|  |  | 1 MiB | **73.72 GiB/s** | 43.49 GiB/s | **1.69x** |
|  |  | 8 MiB | **42.41 GiB/s** | 36.94 GiB/s | **1.15x** |
| Rust batch | XXH3-64 | 32 x 1 KiB | **65.89 GiB/s** | 24.84 GiB/s | **2.65x** |
|  |  | 32 x 4 KiB | **82.72 GiB/s** | 34.80 GiB/s | **2.38x** |
|  | XXH3-128 | 32 x 1 KiB | **59.47 GiB/s** | 19.81 GiB/s | **3.00x** |
|  |  | 32 x 4 KiB | **79.72 GiB/s** | 34.38 GiB/s | **2.32x** |
| Python one-shot | XXH3-64 | 1 KiB | **13.41 GiB/s** | 13.40 GiB/s | 1.00x |
|  |  | 1 MiB | **77.72 GiB/s** | 46.52 GiB/s | **1.67x** |
|  |  | 8 MiB | **46.24 GiB/s** | 38.84 GiB/s | **1.19x** |
|  | XXH3-128 | 1 KiB | 8.82 GiB/s | **9.27 GiB/s** | 0.95x |
|  |  | 1 MiB | **76.94 GiB/s** | 48.47 GiB/s | **1.59x** |
|  |  | 8 MiB | **46.72 GiB/s** | 38.27 GiB/s | **1.22x** |
| Python batch | XXH3-64 | 32 x 1 KiB | **27.57 GiB/s** | 14.55 GiB/s | **1.89x** |
|  |  | 32 x 1 MiB | **36.18 GiB/s** | 18.39 GiB/s | **1.97x** |
|  | XXH3-128 | 32 x 1 KiB | **14.81 GiB/s** | 9.87 GiB/s | **1.50x** |
|  |  | 32 x 1 MiB | **36.29 GiB/s** | 13.75 GiB/s | **2.64x** |

## Reusable Python Buffers

| Alphabet | Input | Operation | hashcodecs |
| --- | --- | --- | ---: |
| Standard | 1 KiB | encode | **11.41 GiB/s** |
|  | 1 KiB | decode | **8.92 GiB/s** |
|  | 4 KiB | encode | **23.65 GiB/s** |
|  | 4 KiB | decode | **16.88 GiB/s** |
|  | 1 MiB | encode | **37.54 GiB/s** |
|  | 1 MiB | decode | **27.56 GiB/s** |
|  | 8 MiB | encode | **17.66 GiB/s** |
|  | 8 MiB | decode | **16.81 GiB/s** |
| URL-safe | 1 KiB | encode | **11.06 GiB/s** |
|  | 1 KiB | decode | **7.41 GiB/s** |
|  | 4 KiB | encode | **23.22 GiB/s** |
|  | 4 KiB | decode | **13.33 GiB/s** |
|  | 1 MiB | encode | **37.65 GiB/s** |
|  | 1 MiB | decode | **19.64 GiB/s** |
|  | 8 MiB | encode | **17.55 GiB/s** |
|  | 8 MiB | decode | **15.09 GiB/s** |

## Python Memoryview Inputs

Full immutable memoryviews reuse their underlying bytes for inputs of at least 64 KiB.

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 1 KiB | encode | 5.43 GiB/s | **6.13 GiB/s** |
|  | decode | 4.77 GiB/s | **5.24 GiB/s** |
| 4 KiB | encode | 17.67 GiB/s | **20.05 GiB/s** |
|  | decode | 13.65 GiB/s | **15.22 GiB/s** |
| 1 MiB | encode | 2.91 GiB/s | **37.46 GiB/s** |
|  | decode | 3.40 GiB/s | **27.52 GiB/s** |
| 8 MiB | encode | 3.01 GiB/s | **17.43 GiB/s** |
|  | decode | 3.79 GiB/s | **17.01 GiB/s** |

## Python Base64 Batches

Each value is total input throughput; the parenthesized figure is items per second. Higher is better.

| Item | Batch | Operation | hashcodecs | hashcodecs loop | pybase64 loop | CPython loop |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 B | 8 | encode | **0.42 GiB/s (28.31M)** | 0.30 GiB/s (19.90M) | 0.12 GiB/s (8.17M) | 0.18 GiB/s (12.09M) |
|  |  | decode | **0.39 GiB/s (25.93M)** | 0.22 GiB/s (14.81M) | 0.08 GiB/s (5.20M) | 0.12 GiB/s (7.80M) |
| 16 B | 64 | encode | **0.56 GiB/s (37.62M)** | 0.33 GiB/s (22.44M) | 0.13 GiB/s (8.70M) | 0.19 GiB/s (13.04M) |
|  |  | decode | **0.51 GiB/s (34.41M)** | 0.24 GiB/s (15.99M) | 0.08 GiB/s (5.54M) | 0.12 GiB/s (8.21M) |
| 16 B | 1,024 | encode | **0.58 GiB/s (38.68M)** | 0.33 GiB/s (21.85M) | 0.13 GiB/s (8.65M) | 0.19 GiB/s (12.70M) |
|  |  | decode | **0.51 GiB/s (34.24M)** | 0.24 GiB/s (15.87M) | 0.08 GiB/s (5.57M) | 0.13 GiB/s (8.41M) |
| 256 B | 8 | encode | **6.05 GiB/s (25.39M)** | 4.33 GiB/s (18.16M) | 1.86 GiB/s (7.78M) | 0.38 GiB/s (1.60M) |
|  |  | decode | **5.42 GiB/s (22.71M)** | 3.26 GiB/s (13.66M) | 1.17 GiB/s (4.91M) | 0.74 GiB/s (3.10M) |
| 256 B | 64 | encode | **7.18 GiB/s (30.13M)** | 4.41 GiB/s (18.50M) | 1.82 GiB/s (7.63M) | 0.39 GiB/s (1.62M) |
|  |  | decode | **6.14 GiB/s (25.75M)** | 3.39 GiB/s (14.21M) | 1.22 GiB/s (5.11M) | 0.75 GiB/s (3.15M) |
| 256 B | 1,024 | encode | **7.17 GiB/s (30.05M)** | 4.34 GiB/s (18.19M) | 1.87 GiB/s (7.86M) | 0.38 GiB/s (1.59M) |
|  |  | decode | **6.11 GiB/s (25.65M)** | 3.27 GiB/s (13.71M) | 1.23 GiB/s (5.15M) | 0.72 GiB/s (3.03M) |
| 4 KiB | 8 | encode | **23.20 GiB/s (6.08M)** | 20.55 GiB/s (5.39M) | 12.23 GiB/s (3.21M) | 0.46 GiB/s (0.12M) |
|  |  | decode | **18.71 GiB/s (4.90M)** | 16.03 GiB/s (4.20M) | 7.74 GiB/s (2.03M) | 1.07 GiB/s (0.28M) |
| 4 KiB | 64 | encode | **17.21 GiB/s (4.51M)** | 15.76 GiB/s (4.13M) | 8.99 GiB/s (2.36M) | 0.45 GiB/s (0.12M) |
|  |  | decode | **18.35 GiB/s (4.81M)** | 15.50 GiB/s (4.06M) | 7.62 GiB/s (2.00M) | 1.06 GiB/s (0.28M) |
| 4 KiB | 1,024 | encode | 1.90 GiB/s (0.50M) | **2.46 GiB/s (0.65M)** | 2.27 GiB/s (0.59M) | 0.39 GiB/s (0.10M) |
|  |  | decode | **13.50 GiB/s (3.54M)** | 11.21 GiB/s (2.94M) | 6.66 GiB/s (1.74M) | 1.05 GiB/s (0.27M) |

## Reusable Python Base64 Batch Buffers

Each value is total input throughput for the `*_batch_into` APIs with one reusable `bytearray` per item.

| Item | Batch | Encode | Decode |
| ---: | ---: | ---: | ---: |
| 16 B | 8 | 0.35 GiB/s | 0.32 GiB/s |
|  | 64 | 0.49 GiB/s | 0.46 GiB/s |
|  | 1,024 | 0.51 GiB/s | 0.48 GiB/s |
| 256 B | 8 | 4.49 GiB/s | 4.22 GiB/s |
|  | 64 | 5.75 GiB/s | 5.56 GiB/s |
|  | 1,024 | 5.85 GiB/s | 5.61 GiB/s |
| 4 KiB | 8 | 24.15 GiB/s | 19.36 GiB/s |
|  | 64 | 25.20 GiB/s | 20.92 GiB/s |
|  | 1,024 | 16.86 GiB/s | 16.21 GiB/s |

## Large Python Base64 Batches

Each value is total input throughput for 1 MiB items.

| Batch | Returned encode | Reusable encode | Returned decode | Reusable decode |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 3.28 GiB/s | **37.26 GiB/s** | 3.39 GiB/s | **27.31 GiB/s** |
| 2 | 2.82 GiB/s | **19.81 GiB/s** | 3.74 GiB/s | **20.63 GiB/s** |
| 4 | 2.80 GiB/s | **18.97 GiB/s** | 3.69 GiB/s | **19.82 GiB/s** |
| 8 | 2.88 GiB/s | **16.38 GiB/s** | 3.41 GiB/s | **15.23 GiB/s** |
| 16 | 2.60 GiB/s | **10.07 GiB/s** | 2.95 GiB/s | **9.38 GiB/s** |
| 32 | 2.52 GiB/s | **8.17 GiB/s** | 3.05 GiB/s | **7.95 GiB/s** |

## Mutable Python Inputs

### Base64

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 1 KiB | encode | 9.88 GiB/s | **11.64 GiB/s** |
|  | decode | 7.43 GiB/s | **8.84 GiB/s** |
| 4 KiB | encode | 20.78 GiB/s | **23.29 GiB/s** |
|  | decode | 15.94 GiB/s | **17.64 GiB/s** |
| 1 MiB | encode | 2.79 GiB/s | **37.70 GiB/s** |
|  | decode | 3.75 GiB/s | **27.65 GiB/s** |
| 8 MiB | encode | 3.05 GiB/s | **17.69 GiB/s** |
|  | decode | 3.87 GiB/s | **16.73 GiB/s** |

### MurmurHash3

| Variant | API | 1 KiB | 4 KiB | 1 MiB | 8 MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| x86 32-bit | one-shot | **3.34 GiB/s** | **3.71 GiB/s** | **3.93 GiB/s** | **3.88 GiB/s** |
|  | incremental | **2.71 GiB/s** | **3.53 GiB/s** | **3.85 GiB/s** | **3.87 GiB/s** |
| x86 128-bit | one-shot | **5.72 GiB/s** | **8.01 GiB/s** | **8.92 GiB/s** | **9.04 GiB/s** |
|  | incremental | **4.02 GiB/s** | **6.91 GiB/s** | **8.76 GiB/s** | **8.92 GiB/s** |
| x64 128-bit | one-shot | **5.82 GiB/s** | **8.39 GiB/s** | **9.83 GiB/s** | **9.84 GiB/s** |
|  | incremental | **4.47 GiB/s** | **7.58 GiB/s** | **9.82 GiB/s** | **9.89 GiB/s** |
