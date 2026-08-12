# Benchmark Details

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and 15 Python samples. Higher is better.

## Reusable Python Buffers

| Alphabet | Input | Operation | hashcodecs |
| --- | --- | --- | ---: |
| Standard | 4 KiB | encode | **15.52 GiB/s** |
|  | 4 KiB | decode | **18.66 GiB/s** |
|  | 1 MiB | encode | **39.91 GiB/s** |
|  | 1 MiB | decode | **29.32 GiB/s** |
|  | 32 MiB | encode | **10.92 GiB/s** |
|  | 32 MiB | decode | **10.40 GiB/s** |
| URL-safe | 4 KiB | encode | **15.11 GiB/s** |
|  | 4 KiB | decode | **14.43 GiB/s** |
|  | 1 MiB | encode | **39.90 GiB/s** |
|  | 1 MiB | decode | **20.69 GiB/s** |
|  | 32 MiB | encode | **10.89 GiB/s** |
|  | 32 MiB | decode | **10.46 GiB/s** |

## Python Memoryview Inputs

Full immutable memoryviews reuse their underlying bytes for inputs of at least 64 KiB.

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 4 KiB | encode | 8.20 GiB/s | **8.66 GiB/s** |
|  | decode | 9.77 GiB/s | **10.30 GiB/s** |
| 1 MiB | encode | 3.48 GiB/s | **39.67 GiB/s** |
|  | decode | 4.28 GiB/s | **29.21 GiB/s** |
| 32 MiB | encode | 2.98 GiB/s | **11.18 GiB/s** |
|  | decode | 3.50 GiB/s | **10.49 GiB/s** |

## Python Base64 Batches

Each value is total input throughput; the parenthesized figure is items per second. Higher is better.

| Item | Batch | Operation | hashcodecs | hashcodecs loop | pybase64 loop | CPython loop |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 B | 8 | encode | **0.38 GiB/s (25.36M)** | 0.25 GiB/s (16.81M) | 0.12 GiB/s (8.22M) | 0.19 GiB/s (12.45M) |
|  |  | decode | **0.35 GiB/s (23.69M)** | 0.20 GiB/s (13.27M) | 0.08 GiB/s (5.35M) | 0.12 GiB/s (7.95M) |
| 16 B | 64 | encode | **0.45 GiB/s (30.25M)** | 0.28 GiB/s (18.62M) | 0.14 GiB/s (9.17M) | 0.21 GiB/s (13.78M) |
|  |  | decode | **0.47 GiB/s (31.62M)** | 0.23 GiB/s (15.41M) | 0.09 GiB/s (5.96M) | 0.13 GiB/s (8.87M) |
| 16 B | 1,024 | encode | **0.48 GiB/s (32.40M)** | 0.27 GiB/s (18.38M) | 0.14 GiB/s (9.09M) | 0.20 GiB/s (13.14M) |
|  |  | decode | **0.48 GiB/s (32.06M)** | 0.23 GiB/s (15.15M) | 0.08 GiB/s (5.69M) | 0.13 GiB/s (8.39M) |
| 256 B | 8 | encode | 1.69 GiB/s (7.07M) | 1.42 GiB/s (5.98M) | **1.85 GiB/s (7.76M)** | 0.40 GiB/s (1.68M) |
|  |  | decode | **4.98 GiB/s (20.89M)** | 2.91 GiB/s (12.20M) | 1.18 GiB/s (4.93M) | 0.74 GiB/s (3.09M) |
| 256 B | 64 | encode | 1.84 GiB/s (7.73M) | 1.47 GiB/s (6.17M) | **1.93 GiB/s (8.10M)** | 0.39 GiB/s (1.64M) |
|  |  | decode | **6.09 GiB/s (25.52M)** | 3.17 GiB/s (13.31M) | 1.28 GiB/s (5.35M) | 0.78 GiB/s (3.28M) |
| 256 B | 1,024 | encode | 1.78 GiB/s (7.47M) | 1.50 GiB/s (6.27M) | **1.87 GiB/s (7.83M)** | 0.37 GiB/s (1.55M) |
|  |  | decode | **5.75 GiB/s (24.10M)** | 2.99 GiB/s (12.55M) | 1.23 GiB/s (5.17M) | 0.72 GiB/s (3.04M) |
| 4 KiB | 8 | encode | **14.24 GiB/s (3.73M)** | 12.84 GiB/s (3.37M) | 12.27 GiB/s (3.22M) | 0.46 GiB/s (0.12M) |
|  |  | decode | **18.68 GiB/s (4.90M)** | 15.88 GiB/s (4.16M) | 7.86 GiB/s (2.06M) | 1.14 GiB/s (0.30M) |
| 4 KiB | 64 | encode | **12.37 GiB/s (3.24M)** | 11.32 GiB/s (2.97M) | 9.57 GiB/s (2.51M) | 0.48 GiB/s (0.13M) |
|  |  | decode | **19.14 GiB/s (5.02M)** | 16.16 GiB/s (4.24M) | 8.09 GiB/s (2.12M) | 1.13 GiB/s (0.30M) |
| 4 KiB | 1,024 | encode | 2.03 GiB/s (0.53M) | **2.46 GiB/s (0.65M)** | 2.35 GiB/s (0.62M) | 0.41 GiB/s (0.11M) |
|  |  | decode | **13.93 GiB/s (3.65M)** | 12.14 GiB/s (3.18M) | 7.13 GiB/s (1.87M) | 1.11 GiB/s (0.29M) |

## Reusable Python Base64 Batch Buffers

Each value is total input throughput for the `*_batch_into` APIs with one reusable `bytearray` per item.

| Item | Batch | Encode | Decode |
| ---: | ---: | ---: | ---: |
| 16 B | 8 | 0.33 GiB/s | 0.30 GiB/s |
|  | 64 | 0.47 GiB/s | 0.44 GiB/s |
|  | 1,024 | 0.47 GiB/s | 0.45 GiB/s |
| 256 B | 8 | 1.57 GiB/s | 4.01 GiB/s |
|  | 64 | 1.73 GiB/s | 5.51 GiB/s |
|  | 1,024 | 1.73 GiB/s | 5.29 GiB/s |
| 4 KiB | 8 | 15.39 GiB/s | 20.29 GiB/s |
|  | 64 | 16.28 GiB/s | 22.12 GiB/s |
|  | 1,024 | 12.95 GiB/s | 16.67 GiB/s |

## Large Python Base64 Batches

Each value is total input throughput for 1 MiB items.

| Batch | Returned encode | Reusable encode | Returned decode | Reusable decode |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 3.52 GiB/s | **39.63 GiB/s** | 3.81 GiB/s | **29.12 GiB/s** |
| 2 | 3.31 GiB/s | **20.90 GiB/s** | 3.99 GiB/s | **21.50 GiB/s** |
| 4 | 3.19 GiB/s | **20.15 GiB/s** | 3.91 GiB/s | **21.00 GiB/s** |
| 8 | 3.17 GiB/s | **19.45 GiB/s** | 3.87 GiB/s | **18.99 GiB/s** |
| 16 | 2.97 GiB/s | **12.96 GiB/s** | 3.68 GiB/s | **12.18 GiB/s** |
| 32 | 2.88 GiB/s | **11.09 GiB/s** | 3.29 GiB/s | **10.60 GiB/s** |

## Mutable Python Inputs

### Base64

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 4 KiB | encode | 13.90 GiB/s | **15.22 GiB/s** |
|  | decode | 16.57 GiB/s | **18.27 GiB/s** |
| 1 MiB | encode | 3.13 GiB/s | **39.59 GiB/s** |
|  | decode | 3.67 GiB/s | **29.24 GiB/s** |
| 32 MiB | encode | 2.99 GiB/s | **11.20 GiB/s** |
|  | decode | 3.65 GiB/s | **10.70 GiB/s** |

### MurmurHash3

| Variant | API | 4 KiB | 1 MiB | 32 MiB |
| --- | --- | ---: | ---: | ---: |
| x86 32-bit | one-shot | **3.79 GiB/s** | **3.98 GiB/s** | **3.97 GiB/s** |
|  | incremental | **3.58 GiB/s** | **3.96 GiB/s** | **3.92 GiB/s** |
| x86 128-bit | one-shot | **8.09 GiB/s** | **9.22 GiB/s** | **9.12 GiB/s** |
|  | incremental | **7.02 GiB/s** | **9.17 GiB/s** | **9.01 GiB/s** |
| x64 128-bit | one-shot | **8.68 GiB/s** | **10.10 GiB/s** | **9.54 GiB/s** |
|  | incremental | **7.70 GiB/s** | **10.01 GiB/s** | **9.41 GiB/s** |
