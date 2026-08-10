# Benchmark Details

Environment: Windows 10 x64 and Intel Core Ultra 7 265K.

Conditions: clean builds, one pinned logical CPU, single-threaded execution, and 15 Python samples. Higher is better.

## Reusable Python Buffers

| Alphabet | Input | Operation | hashcodecs |
| --- | --- | --- | ---: |
| Standard | 4 KiB | encode | **15.36 GiB/s** |
|  | 4 KiB | decode | **18.59 GiB/s** |
|  | 1 MiB | encode | **39.68 GiB/s** |
|  | 1 MiB | decode | **29.08 GiB/s** |
|  | 32 MiB | encode | **11.05 GiB/s** |
|  | 32 MiB | decode | **10.63 GiB/s** |
| URL-safe | 4 KiB | encode | **15.22 GiB/s** |
|  | 4 KiB | decode | **12.32 GiB/s** |
|  | 1 MiB | encode | **39.73 GiB/s** |
|  | 1 MiB | decode | **20.73 GiB/s** |
|  | 32 MiB | encode | **11.20 GiB/s** |
|  | 32 MiB | decode | **10.70 GiB/s** |

## Mutable Python Inputs

### Base64

| Input | Operation | Returned `bytes` | Reusable `bytearray` |
| --- | --- | ---: | ---: |
| 4 KiB | encode | 14.39 GiB/s | **15.30 GiB/s** |
|  | decode | 16.84 GiB/s | **18.38 GiB/s** |
| 1 MiB | encode | 2.98 GiB/s | **39.62 GiB/s** |
|  | decode | 3.73 GiB/s | **29.25 GiB/s** |
| 32 MiB | encode | 2.90 GiB/s | **11.19 GiB/s** |
|  | decode | 3.67 GiB/s | **10.69 GiB/s** |

### MurmurHash3

| Variant | API | 4 KiB | 1 MiB | 32 MiB |
| --- | --- | ---: | ---: | ---: |
| x86 32-bit | one-shot | **3.80 GiB/s** | **3.99 GiB/s** | **3.99 GiB/s** |
|  | incremental | **3.60 GiB/s** | **3.97 GiB/s** | **3.94 GiB/s** |
| x86 128-bit | one-shot | **8.14 GiB/s** | **9.22 GiB/s** | **9.15 GiB/s** |
|  | incremental | **7.06 GiB/s** | **9.19 GiB/s** | **9.05 GiB/s** |
| x64 128-bit | one-shot | **8.83 GiB/s** | **10.14 GiB/s** | **9.62 GiB/s** |
|  | incremental | **7.68 GiB/s** | **10.04 GiB/s** | **9.51 GiB/s** |
