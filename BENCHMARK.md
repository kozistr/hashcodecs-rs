# Benchmark Details

Run the suite on Windows 10 x64 with an Intel Core Ultra 7 265K.

Pin one logical CPU. Run each case in one thread. Collect 50 Rust samples and 15 Python samples. Compile the C
baseline with AVX2, the backend that hashcodecs selects on this host. Higher throughput wins.

Build the Python wheel with CPython 3.12 and the full C API. Keep competitor values from the latest comparison run.
Use `uv run python benchmarks/render_charts.py` to render the charts. Read exact values in
[docs/benchmarks/results.csv](docs/benchmarks/results.csv).

## Timing Controls

Every Python benchmark accepts `--samples` (default: 15) and `--minimum-sample-seconds` (default: 0.2). Their
sampling time per case is at least their product, plus calibration; use lower values only for exploratory runs.

Run the complete Python hashcodecs-only suite at the default timing with:

```sh
uv run --no-project --with . python benchmarks/python_base64.py --hashcodecs-only
uv run --no-project --with . python benchmarks/python_base64_batch.py --hashcodecs-only
uv run --no-project --with . python benchmarks/python_murmur3.py --hashcodecs-only
uv run --no-project --with . python benchmarks/python_xxhash.py --hashcodecs-only
```

For a quicker exploratory pass, append `--samples 3 --minimum-sample-seconds 0.05` to each command.

### Latest hashcodecs-only Python run

The full default-timing run on 2026-08-23, pinned to one logical CPU on the documented benchmark host, included all
cases. Selected throughput results are:

| Case | Input | Throughput |
| --- | ---: | ---: |
| Base64 standard encode | 4 KiB | 23.57 GiB/s |
| Base64 standard decode | 4 KiB | 17.94 GiB/s |
| Base64 batch encode | 256 B × 64 | 8.80 GiB/s |
| Base64 batch decode | 256 B × 64 | 7.58 GiB/s |
| MurmurHash3 x64 128-bit | 1 MiB | 10.07 GiB/s |
| XXH3-64 | 1 MiB | 79.20 GiB/s |
| XXH3-128 | 1 MiB | 79.02 GiB/s |

## XXH3

For the Rust comparison, link hashcodecs with xxHash 0.8.3 through `xxhash-c-sys`. Build the C baseline with AVX2.
For Python, run the upstream `xxhash` extension beside hashcodecs. Pass 32 equal-size inputs to each batch case.

[![Rust XXH3 throughput](docs/benchmarks/xxh3-rust.svg)](docs/benchmarks/xxh3-rust.svg)

[![Python XXH3 throughput](docs/benchmarks/xxh3-python.svg)](docs/benchmarks/xxh3-python.svg)

## Reusable Python Buffers

Pass one reusable `bytearray` to each `*_into` call.

[![Reusable Python Base64 buffers](docs/benchmarks/base64-python-reusable.svg)](docs/benchmarks/base64-python-reusable.svg)

## Python Memoryview Inputs

Use full immutable memoryviews for inputs of at least 64 KiB.

[![Python Base64 memoryview inputs](docs/benchmarks/base64-python-memoryview.svg)](docs/benchmarks/base64-python-memoryview.svg)

## Python Base64 Batches

Set the horizontal axis to batch size. Read total input throughput on the vertical axis.

[![Python Base64 batch throughput](docs/benchmarks/base64-python-batch.svg)](docs/benchmarks/base64-python-batch.svg)

## Reusable Python Base64 Batch Buffers

Pass one reusable `bytearray` to each item in the batch. Use the `*_batch_into` APIs.

[![Reusable Python Base64 batch buffers](docs/benchmarks/base64-python-batch-reusable.svg)](docs/benchmarks/base64-python-batch-reusable.svg)

## Large Python Base64 Batches

Use 1 MiB for each batch item. Set the horizontal axis to batch size.

[![Large Python Base64 batches](docs/benchmarks/base64-python-batch-large.svg)](docs/benchmarks/base64-python-batch-large.svg)

## Mutable Python Inputs

### Base64

Pass `bytearray` inputs to the Base64 API.

[![Mutable Python Base64 inputs](docs/benchmarks/base64-python-mutable.svg)](docs/benchmarks/base64-python-mutable.svg)

### MurmurHash3

Pass `bytearray` inputs to the MurmurHash3 API.

[![Mutable Python MurmurHash3 inputs](docs/benchmarks/murmur3-python-mutable.svg)](docs/benchmarks/murmur3-python-mutable.svg)
