# Benchmark Details

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and 15 Python samples. Higher is better.

## Reusable Python Buffers

| Alphabet | Input | Operation | hashcodecs |
| --- | --- | --- | ---: |
| Standard | 4 KiB | encode | **15.48 GiB/s** |
|  | 4 KiB | decode | **17.20 GiB/s** |
|  | 1 MiB | encode | **19.18 GiB/s** |
|  | 1 MiB | decode | **23.82 GiB/s** |
|  | 32 MiB | encode | **10.95 GiB/s** |
|  | 32 MiB | decode | **10.47 GiB/s** |
| URL-safe | 4 KiB | encode | **15.36 GiB/s** |
|  | 4 KiB | decode | **13.48 GiB/s** |
|  | 1 MiB | encode | **19.20 GiB/s** |
|  | 1 MiB | decode | **18.25 GiB/s** |
|  | 32 MiB | encode | **10.87 GiB/s** |
|  | 32 MiB | decode | **9.88 GiB/s** |

## Python Base64 Batches

Each value is total input throughput; the parenthesized figure is items per second. Higher is better.

| Item | Batch | Operation | hashcodecs | hashcodecs loop | pybase64 loop | CPython loop |
| ---: | ---: | --- | ---: | ---: | ---: | ---: |
| 16 B | 8 | encode | **0.41 GiB/s (27.63M)** | 0.27 GiB/s (18.12M) | 0.13 GiB/s (8.47M) | 0.19 GiB/s (12.56M) |
|  |  | decode | **0.37 GiB/s (24.57M)** | 0.21 GiB/s (14.14M) | 0.08 GiB/s (5.45M) | 0.12 GiB/s (8.00M) |
| 16 B | 64 | encode | **0.55 GiB/s (37.07M)** | 0.31 GiB/s (20.78M) | 0.14 GiB/s (9.20M) | 0.21 GiB/s (13.77M) |
|  |  | decode | **0.49 GiB/s (32.85M)** | 0.25 GiB/s (16.46M) | 0.09 GiB/s (5.92M) | 0.13 GiB/s (8.81M) |
| 16 B | 1,024 | encode | **0.57 GiB/s (38.33M)** | 0.31 GiB/s (21.10M) | 0.14 GiB/s (9.23M) | 0.21 GiB/s (13.76M) |
|  |  | decode | **0.49 GiB/s (33.21M)** | 0.24 GiB/s (16.40M) | 0.09 GiB/s (6.00M) | 0.13 GiB/s (8.83M) |
| 256 B | 8 | encode | **5.29 GiB/s (22.20M)** | 3.67 GiB/s (15.38M) | 1.93 GiB/s (8.11M) | 0.40 GiB/s (1.67M) |
|  |  | decode | **4.90 GiB/s (20.53M)** | 3.02 GiB/s (12.65M) | 1.22 GiB/s (5.12M) | 0.77 GiB/s (3.24M) |
| 256 B | 64 | encode | **6.50 GiB/s (27.28M)** | 4.16 GiB/s (17.45M) | 2.02 GiB/s (8.48M) | 0.41 GiB/s (1.71M) |
|  |  | decode | **5.75 GiB/s (24.12M)** | 3.36 GiB/s (14.09M) | 1.30 GiB/s (5.45M) | 0.80 GiB/s (3.33M) |
| 256 B | 1,024 | encode | **6.55 GiB/s (27.49M)** | 4.14 GiB/s (17.38M) | 1.99 GiB/s (8.36M) | 0.40 GiB/s (1.70M) |
|  |  | decode | **5.82 GiB/s (24.43M)** | 3.30 GiB/s (13.86M) | 1.30 GiB/s (5.47M) | 0.76 GiB/s (3.20M) |
| 4 KiB | 8 | encode | **10.47 GiB/s (2.74M)** | 9.89 GiB/s (2.59M) | 9.41 GiB/s (2.47M) | 0.48 GiB/s (0.13M) |
|  |  | decode | **17.00 GiB/s (4.46M)** | 15.03 GiB/s (3.94M) | 8.24 GiB/s (2.16M) | 1.14 GiB/s (0.30M) |
| 4 KiB | 64 | encode | **12.56 GiB/s (3.29M)** | 11.60 GiB/s (3.04M) | 9.60 GiB/s (2.52M) | 0.48 GiB/s (0.13M) |
|  |  | decode | **16.87 GiB/s (4.42M)** | 14.74 GiB/s (3.86M) | 8.13 GiB/s (2.13M) | 1.13 GiB/s (0.30M) |
| 4 KiB | 1,024 | encode | 5.90 GiB/s (1.55M) | **9.34 GiB/s (2.45M)** | 7.52 GiB/s (1.97M) | 0.47 GiB/s (0.12M) |
|  |  | decode | **5.85 GiB/s (1.53M)** | 4.45 GiB/s (1.17M) | 3.47 GiB/s (0.91M) | 0.96 GiB/s (0.25M) |

## Mutable Python Inputs

### Base64

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 4 KiB | encode | 13.93 GiB/s | **14.71 GiB/s** |
|  | decode | 15.39 GiB/s | **16.50 GiB/s** |
| 1 MiB | encode | 3.56 GiB/s | **18.70 GiB/s** |
|  | decode | 3.50 GiB/s | **23.90 GiB/s** |
| 32 MiB | encode | 3.70 GiB/s | **10.50 GiB/s** |
|  | decode | 3.75 GiB/s | **10.03 GiB/s** |

### MurmurHash3

| Variant | API | 4 KiB | 1 MiB | 32 MiB |
| --- | --- | ---: | ---: | ---: |
| x86 32-bit | one-shot | **3.80 GiB/s** | **3.99 GiB/s** | **3.99 GiB/s** |
|  | incremental | **3.60 GiB/s** | **3.97 GiB/s** | **3.94 GiB/s** |
| x86 128-bit | one-shot | **8.14 GiB/s** | **9.22 GiB/s** | **9.15 GiB/s** |
|  | incremental | **7.06 GiB/s** | **9.19 GiB/s** | **9.05 GiB/s** |
| x64 128-bit | one-shot | **8.83 GiB/s** | **10.14 GiB/s** | **9.62 GiB/s** |
|  | incremental | **7.68 GiB/s** | **10.04 GiB/s** | **9.51 GiB/s** |
