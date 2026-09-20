# Benchmarks

Use this reference to compare `hashcodecs` APIs and reproduce the measurements for a change. For the implementation
behind the results, read [Architecture](docs/ARCHITECTURE.md).

## Measurement conditions

| Setting | Recorded comparison |
| --- | --- |
| Host | Windows 10 x64, Intel Core Ultra 7 265K |
| Execution | One thread, pinned to one logical CPU |
| Rust | Criterion, 50 samples per case, release optimization |
| Python | CPython 3.12.10, median of 15 samples, calibrated to at least 0.2 seconds per sample |
| Allocation | `mimalloc` for Rust benchmark allocations and Rust allocations in the Python extension |
| XXH3 C baseline | xxHash 0.8.3 through `xxhash-c-sys`, compiled with AVX2 to match this host's selected backend |

Charts report GiB/s (2³⁰ bytes per second); higher values mean greater throughput. Base64 uses the original binary
payload size for both encoding and decoding. Batch throughput counts the total payload across items. Default
Python Base64 comparisons use immutable `bytes` and strict decoding (`validate=True`).

Returned-output cases include result allocation. Reusable-output cases allocate their destinations before timing.
The harnesses reuse inputs across iterations, so cache residency affects the results. Compare like input types,
sizes, and output models; these measurements describe this host and workload.

The checked-in [results.csv](docs/benchmarks/results.csv) supplies the chart values. Focused updates replace the
affected series and retain the other measurements, so the charts combine results from multiple runs.

## Base64 results

| API and workload | Charts |
| --- | --- |
| Rust, standard and URL-safe alphabets | [Rust Base64](docs/benchmarks/base64-rust.svg) |
| Python, standard and URL-safe alphabets | [Python Base64](docs/benchmarks/base64-python.svg) |
| Python, reusable `bytearray` output | [Reusable buffers](docs/benchmarks/base64-python-reusable.svg) |
| Python, MIME whitespace and ignored non-alphabet characters | [Lenient decoding](docs/benchmarks/base64-python-lenient.svg) |
| Python, full and sliced immutable views | [Memoryview inputs](docs/benchmarks/base64-python-memoryview.svg) |
| Python, ASCII `str` decoding | [String inputs](docs/benchmarks/base64-python-str.svg) |
| Python, mutable `bytearray` input | [Mutable inputs](docs/benchmarks/base64-python-mutable.svg) |
| Python, batches | [Returned bytes](docs/benchmarks/base64-python-batch.svg), [reusable outputs](docs/benchmarks/base64-python-batch-reusable.svg) |
| Python, batches of memoryviews | [Memoryview batches](docs/benchmarks/base64-python-batch-memoryview.svg) |
| Python, batches of 1 MiB items | [Large batches](docs/benchmarks/base64-python-batch-large.svg) |

Batch charts use item count on the horizontal axis. Reusable Base64 batches provide one destination per item.
Lenient cases insert CRLF or `!` after each 76-character line.

## MurmurHash3 results

| API and workload | Charts |
| --- | --- |
| Rust, x86-32, x86-128, and x64-128 | [Rust MurmurHash3](docs/benchmarks/murmur3-rust.svg) |
| Python, one-shot and incremental calls | [Python MurmurHash3](docs/benchmarks/murmur3-python.svg) |
| Python, mutable `bytearray` input | [Mutable inputs](docs/benchmarks/murmur3-python-mutable.svg) |

Incremental cases include construction, `update`, and digest creation.

## XXH3 results

| API and workload | Charts |
| --- | --- |
| Rust, XXH3-64 and XXH3-128, one-shot and 32-item batches | [Rust XXH3](docs/benchmarks/xxh3-rust.svg) |
| Rust, two- and three-item batches | [Batch remainders](docs/benchmarks/xxh3-rust-batch-remainders.svg) |
| Python, one-shot, list results, and packed output | [Python XXH3](docs/benchmarks/xxh3-python.svg) |

Rust allocating batches include result-vector allocation. Python list batches create one integer per digest;
packed batches write little-endian digests into one reusable `bytearray`. Python batch comparisons use 32
equal-size inputs by default and compare against the upstream `xxhash` extension.

## Reproduce a benchmark

Run commands from the repository root. Install Rust 1.89 or newer, a C/C++ compiler and linker for your platform,
and `uv`. Use CPython 3.12 for the chart comparisons. The Python setup below builds and installs the current
checkout as a CPython wheel through Hatchling.

Run the group affected by your change. Reserve a complete run for changes that can affect all groups. Keep
benchmarks out of CI. The standard harnesses set CPU affinity on Windows and Linux; on other platforms, arrange
equivalent CPU pinning before collecting comparison results.

### Rust

Choose the relevant harness:

```sh
cargo bench --manifest-path benches/Cargo.toml --bench base64
cargo bench --manifest-path benches/Cargo.toml --bench murmur3
cargo bench --manifest-path benches/Cargo.toml --bench xxhash
```

To reproduce the Windows XXH3 C baseline, set the compiler flags and rebuild its package before running the harness:

```powershell
$env:CFLAGS = '/O2 /arch:AVX2'
cargo clean -p xxhash-c-sys
cargo bench --manifest-path benches/Cargo.toml --bench xxhash
```

Use the equivalent AVX2 compiler flag on other platforms. Match the baseline instruction set to the backend under
comparison. Pass a Criterion name filter after `--` to narrow a run:

```sh
cargo bench --manifest-path benches/Cargo.toml --bench murmur3 -- x64_128/hashcodecs
```

### Python

Prepare the environment once, and repeat the wheel installation after changing native code:

```sh
uv sync --python 3.12 --frozen --group benchmark --no-install-project
uv run --python 3.12 --frozen --no-sync python tools/install_local_wheel.py
```

Choose the relevant script:

```sh
uv run --python 3.12 --frozen --no-sync python benchmarks/python_base64.py
uv run --python 3.12 --frozen --no-sync python benchmarks/python_base64_batch.py
uv run --python 3.12 --frozen --no-sync python benchmarks/python_murmur3.py
uv run --python 3.12 --frozen --no-sync python benchmarks/python_xxhash.py
```

The scripts check outputs before timing and print GiB/s. Append a mode from the table to select a workload; use
`--help` for the full option list.

| Script | Focused modes |
| --- | --- |
| `python_base64.py` | `--into`, `--lenient`, `--custom-lenient`, `--bytearray-input`, `--memoryview-input`, `--sliced-memoryview-input`, `--str-input` |
| `python_base64_batch.py` | `--large`, `--memoryview-input`, `--decode-only`; select sizes with `--item-sizes` and `--batch-sizes` |
| `python_murmur3.py` | `--incremental`, `--bytearray-input` |
| `python_xxhash.py` | `--batches-only`, `--batch-counts 2 3` |

`python_base64.py` accepts one mode per run. Its `--configured` and `--wrapped` modes require CPython 3.15 or newer;
use that interpreter for both setup commands and the benchmark. These modes cover configured decoding and
encoding with newlines after 76 output characters.

The four scripts accept `--hashcodecs-only` to skip competitor timing. The Base64 script treats that flag as a
mode, so run it apart from `--into`, `--lenient`, and the other Base64 modes. All four accept `--samples` and
`--minimum-sample-seconds`. Keep the defaults for published comparisons; lower them for local exploration.

For call overhead, run `benchmarks/python_calls.py` with the same `uv run` prefix. It reports nanoseconds per call
and supports `--keywords`, `--thresholds`, and `--buffer-inputs`. Its `--thread-scaling` mode measures aggregate
throughput without single-CPU pinning; keep those results separate from the charts above.

## Update the charts

1. Copy the measured GiB/s values into [results.csv](docs/benchmarks/results.csv). Update the series you measured.
   Retain unmeasured values and leave `gib_per_second` empty for unavailable results.
2. Keep matching input categories across implementations in each panel. Each row identifies a chart, panel,
   input category, and implementation; row order controls chart, panel, category, and legend order.
3. Render the SVG files:

   ```sh
   uv run --python 3.12 --no-project python benchmarks/render_charts.py
   ```

4. Review the CSV and SVG diff. A focused update should change the corresponding charts, including any README
   overview that uses those values.

The renderer reads the CSV without running benchmarks or changing measurements. Keep one-time tuning sweeps,
branch comparisons, and profiler output out of this reference.
