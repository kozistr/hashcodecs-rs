# Benchmark Details

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and 15 Python samples. Higher is better.

## Reusable Python Buffers

| Alphabet | Input | Operation | hashcodecs |
| --- | --- | --- | ---: |
| Standard | 4 KiB | encode | **15.09 GiB/s** |
|  | 4 KiB | decode | **18.43 GiB/s** |
|  | 1 MiB | encode | **39.66 GiB/s** |
|  | 1 MiB | decode | **29.03 GiB/s** |
|  | 32 MiB | encode | **10.89 GiB/s** |
|  | 32 MiB | decode | **10.36 GiB/s** |
| URL-safe | 4 KiB | encode | **14.83 GiB/s** |
|  | 4 KiB | decode | **12.26 GiB/s** |
|  | 1 MiB | encode | **39.71 GiB/s** |
|  | 1 MiB | decode | **20.72 GiB/s** |
|  | 32 MiB | encode | **10.84 GiB/s** |
|  | 32 MiB | decode | **10.44 GiB/s** |

## Python Base64 Batches

Each value is total input throughput; the parenthesized figure is items per second. Higher is better.

| Item | Batch | Operation | hashcodecs | hashcodecs loop | pybase64 loop | CPython loop |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 B | 8 | encode | **0.41 GiB/s (27.42M)** | 0.27 GiB/s (18.11M) | 0.13 GiB/s (8.45M) | 0.18 GiB/s (12.37M) |
|  |  | decode | **0.36 GiB/s (24.24M)** | 0.21 GiB/s (14.13M) | 0.08 GiB/s (5.44M) | 0.12 GiB/s (7.85M) |
| 16 B | 64 | encode | **0.55 GiB/s (36.69M)** | 0.31 GiB/s (20.81M) | 0.14 GiB/s (9.17M) | 0.21 GiB/s (13.81M) |
|  |  | decode | **0.49 GiB/s (32.86M)** | 0.24 GiB/s (16.13M) | 0.09 GiB/s (5.87M) | 0.13 GiB/s (8.63M) |
| 16 B | 1,024 | encode | **0.56 GiB/s (37.26M)** | 0.31 GiB/s (20.64M) | 0.14 GiB/s (9.07M) | 0.20 GiB/s (13.73M) |
|  |  | decode | **0.49 GiB/s (32.70M)** | 0.25 GiB/s (16.60M) | 0.09 GiB/s (5.95M) | 0.13 GiB/s (8.71M) |
| 256 B | 8 | encode | 1.77 GiB/s (7.43M) | 1.52 GiB/s (6.38M) | **1.93 GiB/s (8.08M)** | 0.40 GiB/s (1.67M) |
|  |  | decode | **5.03 GiB/s (21.12M)** | 3.02 GiB/s (12.66M) | 1.22 GiB/s (5.12M) | 0.76 GiB/s (3.19M) |
| 256 B | 64 | encode | 1.92 GiB/s (8.05M) | 1.62 GiB/s (6.82M) | **2.02 GiB/s (8.48M)** | 0.41 GiB/s (1.71M) |
|  |  | decode | **6.09 GiB/s (25.56M)** | 3.36 GiB/s (14.07M) | 1.29 GiB/s (5.40M) | 0.76 GiB/s (3.18M) |
| 256 B | 1,024 | encode | 1.92 GiB/s (8.05M) | 1.63 GiB/s (6.82M) | **1.99 GiB/s (8.35M)** | 0.40 GiB/s (1.69M) |
|  |  | decode | **5.87 GiB/s (24.61M)** | 3.30 GiB/s (13.83M) | 1.29 GiB/s (5.43M) | 0.76 GiB/s (3.17M) |
| 4 KiB | 8 | encode | **15.12 GiB/s (3.96M)** | 13.72 GiB/s (3.60M) | 12.69 GiB/s (3.33M) | 0.48 GiB/s (0.13M) |
|  |  | decode | **19.20 GiB/s (5.03M)** | 16.52 GiB/s (4.33M) | 8.29 GiB/s (2.17M) | 1.13 GiB/s (0.30M) |
| 4 KiB | 64 | encode | **12.59 GiB/s (3.30M)** | 11.55 GiB/s (3.03M) | 9.54 GiB/s (2.50M) | 0.48 GiB/s (0.12M) |
|  |  | decode | **18.95 GiB/s (4.97M)** | 16.12 GiB/s (4.23M) | 8.12 GiB/s (2.13M) | 1.12 GiB/s (0.29M) |
| 4 KiB | 1,024 | encode | 5.84 GiB/s (1.53M) | **8.81 GiB/s (2.31M)** | 7.83 GiB/s (2.05M) | 0.47 GiB/s (0.12M) |
|  |  | decode | 3.52 GiB/s (0.92M) | **3.81 GiB/s (1.00M)** | 2.99 GiB/s (0.79M) | 0.92 GiB/s (0.24M) |

## Mutable Python Inputs

### Base64

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 4 KiB | encode | 14.12 GiB/s | **15.16 GiB/s** |
|  | decode | 16.76 GiB/s | **18.27 GiB/s** |
| 1 MiB | encode | 2.88 GiB/s | **38.96 GiB/s** |
|  | decode | 3.59 GiB/s | **28.99 GiB/s** |
| 32 MiB | encode | 2.92 GiB/s | **10.83 GiB/s** |
|  | decode | 3.44 GiB/s | **10.35 GiB/s** |

### MurmurHash3

| Variant | API | 4 KiB | 1 MiB | 32 MiB |
| --- | --- | ---: | ---: | ---: |
| x86 32-bit | one-shot | **3.79 GiB/s** | **3.98 GiB/s** | **3.97 GiB/s** |
|  | incremental | **3.58 GiB/s** | **3.96 GiB/s** | **3.92 GiB/s** |
| x86 128-bit | one-shot | **8.09 GiB/s** | **9.22 GiB/s** | **9.12 GiB/s** |
|  | incremental | **7.02 GiB/s** | **9.17 GiB/s** | **9.01 GiB/s** |
| x64 128-bit | one-shot | **8.68 GiB/s** | **10.10 GiB/s** | **9.54 GiB/s** |
|  | incremental | **7.70 GiB/s** | **10.01 GiB/s** | **9.41 GiB/s** |
