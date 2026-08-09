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
