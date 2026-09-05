# Benchmark Details

Run the suite on Windows 10 x64 with an Intel Core Ultra 7 265K.

Pin one logical CPU. Run each case in one thread. Collect 50 Rust samples and 15 Python samples. Compile the C
baseline with AVX2, the backend that hashcodecs selects on this host. Higher throughput wins.

Build the Python wheel with CPython 3.12 and the full C API. Keep competitor values from the latest comparison run.
Use `uv run --python 3.12 --no-project python benchmarks/render_charts.py` to render the charts. Read exact values in
[docs/benchmarks/results.csv](docs/benchmarks/results.csv).

## Timing Controls

Every Python benchmark accepts `--samples` (default: 15) and `--minimum-sample-seconds` (default: 0.2). Their
sampling time per case is at least their product, plus calibration; use lower values only for exploratory runs. For a quicker full
hashcodecs-only pass, use `--hashcodecs-only --samples 3 --minimum-sample-seconds 0.05` with each Python benchmark
script.

Each Rust Criterion harness collects 50 samples per case. Pass Criterion's `--sample-size` option for an exploratory
run with a different count.

## Python Call Costs

Run `python benchmarks/python_calls.py` to measure positional calls from 0 through 256 bytes in nanoseconds per
call. Use `--keywords` for positional and keyword calls at 64 bytes, or `--thresholds` for latency around the
GIL-detachment cutoffs. The `--thread-scaling` mode measures aggregate throughput with one, two, and four threads;
it does not pin the process to one logical CPU. Use `--buffer-inputs` to compare 64-byte and 4 KiB XXH3-64 calls
across bytes, full and sliced memoryviews, writable and non-contiguous views, and `array('B')`.

## XXH3

For the Rust comparison, link hashcodecs with xxHash 0.8.3 through `xxhash-c-sys`. Build the C baseline with AVX2.
For Python, run the upstream `xxhash` extension beside hashcodecs. Pass 32 equal-size inputs to each batch case.
The Rust remainder cases pass two or three equal-size long inputs. Run Python remainder cases with
`python benchmarks/python_xxhash.py --batch-counts 2 3`.

To measure a branch against its parent, build both extensions with the same Python and Rust toolchains, then run:

```sh
uv run --frozen --no-sync python benchmarks/compare_xxh3_batches.py path/to/parent/_hashcodecs.pyd path/to/branch/_hashcodecs.pyd
```

Use the corresponding `.so` paths on Linux or macOS. The comparison loads both builds into one interpreter, pins
one CPU, and alternates their timing order across 15 samples. It checks matching digests and covers bytes,
bytearrays, and writable memoryviews. Use `--batch-counts 2 9 32 33 --sizes 64` to inspect small batches and the
32-result stack boundary. Positive `change_percent` values mean higher branch throughput.

The [32-item parent comparison](docs/benchmarks/xxh3-batch-parent-comparison.csv) records CPython 3.12.10 results
against parent commit `f17ab86`, measured on 2026-09-05. These paired measurements also cover the 256 KiB
GIL-detachment threshold and 1 MiB items. The Python XXH3 chart uses the candidate's allocating-batch measurements;
packed-output and upstream values retain their prior measurements.

Across the 30 paired 32-item cases, branch throughput ranges from 2.59% lower to 2.70% higher than the parent.
The [stack-boundary comparison](docs/benchmarks/xxh3-batch-boundary-comparison.csv) covers counts 2 and 33 with
64-byte items: two-item bytearray batches lose 5.40–6.05%, and 33-item bytes batches lose 6.06–7.05%. These
measurements show residual overhead for some small-input batches; they do not establish zero regression.

The Rust mixed benchmarks use `[1024, 1024, 4096, 4096]`, `[240, 240, 241, 241]`, and the reverse boundary order.
The 1024/4096 case measures adjacent two-item long runs. The 240/241 cases measure both orders across the
short/long dispatch boundary.

Use the focused one-shot run to cover the AVX2 four-chain boundaries:

```sh
cargo bench --manifest-path benches/Cargo.toml --bench xxhash -- "xxh3_(64|128)/(240|241|512|768|1024|1536|2048|4096)/hashcodecs"
cargo bench --manifest-path benches/Cargo.toml --bench xxhash -- "xxh3_batch/mixed/.*/hashcodecs_(64|128)"
```

[![Rust XXH3 throughput](docs/benchmarks/xxh3-rust.svg)](docs/benchmarks/xxh3-rust.svg)

[![Rust XXH3 batch remainder throughput](docs/benchmarks/xxh3-rust-batch-remainders.svg)](docs/benchmarks/xxh3-rust-batch-remainders.svg)

[![Python XXH3 throughput](docs/benchmarks/xxh3-python.svg)](docs/benchmarks/xxh3-python.svg)

## Reusable Python Buffers

Pass one reusable `bytearray` to each `*_into` call.

[![Reusable Python Base64 buffers](docs/benchmarks/base64-python-reusable.svg)](docs/benchmarks/base64-python-reusable.svg)

## Lenient Python Base64

Run `python benchmarks/python_base64.py --lenient`. The MIME cases insert CRLF after each 76-character line. The
noisy cases insert `!` at the same boundaries. Both cases measure returned bytes and reusable output buffers.

[![Lenient Python Base64 throughput](docs/benchmarks/base64-python-lenient.svg)](docs/benchmarks/base64-python-lenient.svg)

## Python Memoryview Inputs

Use `--memoryview-input` for full immutable views and `--sliced-memoryview-input` for equal-length contiguous views
with a nonzero starting offset. Full views can recover their exact immutable owner at detachment sizes; slices cover
offset-buffer handling, which borrows under the GIL and stabilizes the input in free-threaded builds. The encoded data
remains identical.

[![Python Base64 memoryview inputs](docs/benchmarks/base64-python-memoryview.svg)](docs/benchmarks/base64-python-memoryview.svg)

## Python Base64 Batches

Set the horizontal axis to batch size. Read total input throughput on the vertical axis.

[![Python Base64 batch throughput](docs/benchmarks/base64-python-batch.svg)](docs/benchmarks/base64-python-batch.svg)

For focused runs, override the item and batch sizes directly. `--decode-only` avoids carrying encode allocator state
into a decode investigation:

```sh
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 512 768 1024 1280 2048 --decode-only
```

Add `--memoryview-input` to wrap every matrix input in an exact memoryview. This mode compares independent views
against the matching one-item loops and reusable-output paths.

[![Python Base64 memoryview batch throughput](docs/benchmarks/base64-python-batch-memoryview.svg)](docs/benchmarks/base64-python-batch-memoryview.svg)

```sh
python benchmarks/python_base64_batch.py --item-sizes 1048576 --batch-sizes 8 --memoryview-input --decode-only
```

Use a single operation when recording a sampling profile, or compare traced allocations without a sampler:

```sh
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 1024 --profile-operation returned
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 1024 --allocation-profile
```

Add `--profile-direction encode` for returned encoding. By default, the profiling loop assigns the next result
before releasing the previous one. Add `--discard-profile-result` to release each result before the next call. Use
`b64encode_batch_into` for that workload.

```sh
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 1024 --profile-direction encode --profile-operation returned
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 1024 --profile-direction encode --profile-operation returned --discard-profile-result
python benchmarks/python_base64_batch.py --item-sizes 4096 --batch-sizes 1024 --profile-direction encode --allocation-profile
```

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

## Reproduction

Run the benchmark

```
uv sync --python 3.12 --frozen --group benchmark --no-install-project

uv run --python 3.12 --refresh-package hashcodecs --no-project --with . --with mmh3==5.2.1 --with pybase64==1.4.3 --with xxhash==3.8.1 python benchmarks/python_base64.py --hashcodecs-only

uv run --python 3.12 --refresh-package hashcodecs --no-project --with . --with mmh3==5.2.1 --with pybase64==1.4.3 --with xxhash==3.8.1 python benchmarks/python_base64_batch.py --hashcodecs-only

uv run --python 3.12 --refresh-package hashcodecs --no-project --with . --with mmh3==5.2.1 --with pybase64==1.4.3 --with xxhash==3.8.1 python benchmarks/python_murmur3.py --hashcodecs-only

uv run --python 3.12 --refresh-package hashcodecs --no-project --with . --with mmh3==5.2.1 --with pybase64==1.4.3 --with xxhash==3.8.1 python benchmarks/python_murmur3.py --hashcodecs-only --incremental

uv run --python 3.12 --refresh-package hashcodecs --no-project --with . --with mmh3==5.2.1 --with pybase64==1.4.3 --with xxhash==3.8.1 python benchmarks/python_xxhash.py --hashcodecs-only
```

Update the documentation

```
uv run --python 3.12 --no-project python benchmarks/render_charts.py
```
