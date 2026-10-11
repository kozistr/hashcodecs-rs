# Benchmarks

Use this reference to compare `hashcodecs` APIs and reproduce the measurements for a change. For the implementation
behind the results, read [Architecture](docs/ARCHITECTURE.md).

## Measurement conditions

| Setting | Recorded comparison |
| --- | --- |
| Host | Windows 10 x64, Intel Core Ultra 7 265K |
| Execution | One thread, pinned to one logical CPU |
| Rust | Criterion, 50 samples per case, release optimization |
| Python timing | Median of 15 samples, calibrated to at least 0.2 seconds per sample |
| Python | Free-threaded CPython 3.15.0 (`3.15t`), GIL disabled |
| Python baselines | pybase64 1.5.0, mmh3 5.3.0, xxhash 4.0.1 |
| Allocation | `mimalloc` for Rust benchmark allocations and Rust allocations in the Python extension |
| XXH3 C baseline | xxHash 0.8.3 through `xxhash-c-sys`, compiled with AVX2 to match this host's selected backend |

Charts report GiB/s (2³⁰ bytes per second). Higher values mean greater throughput. Base64 uses the original binary
payload size for both encoding and decoding. Batch throughput counts the total payload across items. Default
Python Base64 comparisons use immutable `bytes` and strict decoding (`validate=True`).

Cases that return output include result allocation. Cases with reusable output allocate destinations before timing.
The harnesses reuse inputs across iterations, so cache residency affects the results. Compare like input types,
sizes, and output models. These measurements describe this host and workload.

The repository's [results.csv](docs/benchmarks/results.csv) supplies the chart values. Focused updates replace the
affected series and retain the other measurements, so the charts combine results from multiple runs.

## Base64 results

| API and workload | Charts |
| --- | --- |
| Rust, standard Base64 and Base64 for URLs | [Rust Base64](docs/benchmarks/base64-rust.svg) |
| Python, standard Base64 and Base64 for URLs | [Python Base64](docs/benchmarks/base64-python.svg) |
| Python, reusable `bytearray` output | [Reusable buffers](docs/benchmarks/base64-python-reusable.svg) |
| Python, MIME whitespace and ignored characters outside the alphabet | [Lenient decoding](docs/benchmarks/base64-python-lenient.svg) |
| Python, full and sliced immutable views | [Memoryview inputs](docs/benchmarks/base64-python-memoryview.svg) |
| Python, ASCII `str` decoding | [String inputs](docs/benchmarks/base64-python-str.svg) |
| Python, mutable `bytearray` input | [Mutable inputs](docs/benchmarks/base64-python-mutable.svg) |
| Python, batches | [Returned bytes](docs/benchmarks/base64-python-batch.svg), [reusable outputs](docs/benchmarks/base64-python-batch-reusable.svg) |
| Python, batches of memoryviews | [Memoryview batches](docs/benchmarks/base64-python-batch-memoryview.svg) |
| Python, batches of 1 MiB items | [Large batches](docs/benchmarks/base64-python-batch-large.svg) |

Batch charts use item count on the horizontal axis. Reusable Base64 batches provide one destination per item.
Lenient cases insert CRLF or `!` after each line of 76 characters.

## MurmurHash3 results

| API and workload | Charts |
| --- | --- |
| Rust, x86-32, x86-128, and x64-128 | [Rust MurmurHash3](docs/benchmarks/murmur3-rust.svg) |
| Python, single calls and incremental hashing | [Python MurmurHash3](docs/benchmarks/murmur3-python.svg) |
| Python, mutable `bytearray` input | [Mutable inputs](docs/benchmarks/murmur3-python-mutable.svg) |

Incremental cases include construction, `update`, and digest creation.

## XXH3 results

| API and workload | Charts |
| --- | --- |
| Rust, XXH3-64 and XXH3-128, single calls and batches of 32 items | [Rust XXH3](docs/benchmarks/xxh3-rust.svg) |
| Rust, batches of two and three items | [Batch remainders](docs/benchmarks/xxh3-rust-batch-remainders.svg) |
| Python, single calls, list results, and packed output | [Python XXH3](docs/benchmarks/xxh3-python.svg) |

Rust allocating batches include allocation of the result vector. Python list batches create one integer per digest.
Packed batches write digests in little endian order into one reusable `bytearray`. Python batch comparisons use
32 inputs of equal size by default and compare against the upstream `xxhash` extension.

The Python small-input panels cover 16, 17, 33, 65, 97, and 240 bytes. These sizes use scalar formulas; runtime SIMD
dispatch starts at 241 bytes.

## Reproduce a benchmark

Run commands from the repository root. Install Rust 1.89 or newer, a C/C++ compiler and linker for your platform,
and `uv`. Use free-threaded CPython 3.15.0 for the Python charts. The Python setup below builds and installs the
current checkout as a CPython wheel through Hatchling.

Run the group affected by your change. Reserve a complete run for changes that can affect all groups. Keep
benchmarks out of CI. The standard harnesses set CPU affinity on Windows and Linux. On other platforms, arrange
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

Prepare the free-threaded interpreter (`3.15.0t`) and pinned benchmark dependencies. If your `uv` release does not
provide this interpreter yet, pass the path to an installed CPython 3.15.0t executable with `--python`. Repeat the
wheel installation after changing native code or switching interpreters:

```sh
uv sync --python 3.15.0t --frozen --group benchmark --no-install-project
uv run --python 3.15.0t --frozen --no-sync python tools/install_local_wheel.py
```

Choose the relevant script:

```sh
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_base64.py
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_base64_batch.py
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_murmur3.py
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_xxhash.py
```

For a complete chart refresh, use `benchmarks/python_suite.py`. It selects the 17 runs needed for all 502 Python
measurements, skips unpublished diagnostics and unused competitor comparisons, and writes progress, logs, and
a command manifest under `target/python-benchmarks/publication`. Select groups or preview commands before running:

```sh
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_suite.py --list
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_suite.py --groups base64
```

The scripts check outputs before timing and print GiB/s. Append a mode from the table to select a workload. Use
`--help` for the full option list.

| Script | Focused modes |
| --- | --- |
| `python_base64.py` | `--into`, `--lenient`, `--custom-lenient`, `--bytearray-input`, `--memoryview-input`, `--sliced-memoryview-input`, `--str-input`, `--sizes BYTES ...` |
| `python_base64_batch.py` | `--large`, `--memoryview-input`, `--decode-only`. Select sizes with `--item-sizes` and `--batch-sizes`. |
| `python_murmur3.py` | `--incremental`, `--bytearray-input`, `--sizes BYTES ...` |
| `python_xxhash.py` | `--one-shot-only`, `--sizes 17 33 65 97 240`, `--batches-only`, `--batch-counts 2 3`, `--thresholds` |

`python_base64.py` accepts one mode per run. Its `--configured` and `--wrapped` modes require CPython 3.15 or newer.
Use that interpreter for both setup commands and the benchmark. These modes cover configured decoding and
encoding with newlines after 76 output characters.

The four scripts and the suite accept `--hashcodecs-only` to skip competitor timing. Combine it with any workload
mode, including `--into` and `--lenient`. The complete suite then measures 324 hashcodecs cases. All four scripts
and the suite accept `--samples` and `--minimum-sample-seconds`. Keep the defaults for published comparisons:
15 samples of at least 0.2 seconds.
That is at least 25 minutes of sampling for a complete chart refresh, before calibration and setup. Calibration
estimates the iteration count from elapsed time instead of rounding to the next power of two. Batch comparisons
reuse the returned-output measurement from the same case instead of timing it again beside reusable output.

Use `--quick` for local exploration: 5 samples of 0.03 seconds. Explicit timing arguments override that preset.
Quick output belongs in `target/python-benchmarks/quick` and must not replace published chart measurements.

```sh
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_suite.py --quick
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_suite.py --quick --hashcodecs-only --workers
uv run --python 3.15.0t --frozen --no-sync python benchmarks/python_base64.py --into --sizes 1048576 --quick
```

The scripts accept `--cpu N` to pin a chosen logical CPU on Windows or Linux. For parallel exploration, the suite's
`--quick --workers` selects up to four physical cores; `--workers N` sets another limit. Selection respects process
affinity, prefers the highest available core class, and excludes parked or externally allocated Windows CPUs.
It samples load for 0.5 seconds and requires every online SMT sibling on a selected core to be at most 20% busy.
It uses fewer workers when necessary and reports an error if no suitable core is idle. Windows automatic
selection currently requires one processor group. Use `--list` to preview the selected CPUs and workloads.

Alternatively, `--quick --cpus 0 1` uses specific logical CPUs. Choose separate physical cores of the same type;
adjacent CPU numbers need not identify equivalent cores. The run manifest records the CPU choices. Publication
runs remain serial. Automatic selection is a recent-load check, not a CPU reservation: idle cores still share
cache, memory bandwidth, power limits, and turbo frequency. Validate serially before publishing.

For call overhead, run `benchmarks/python_calls.py` with the same `uv run` prefix. It reports nanoseconds per call
and supports `--keywords`, `--thresholds`, and `--buffer-inputs`. Its `--thread-scaling` mode measures aggregate
throughput without pinning to one CPU. Use `--sizes BYTES ...` to narrow positional or threshold measurements.
Keep those results separate from the charts above.

For build comparisons, use `benchmarks/compare.py base64 BASELINE CANDIDATE` or
`benchmarks/compare.py xxh3 BASELINE CANDIDATE`, with paths to the two built extension modules. These replace the
separate Base64 and XXH3 comparison scripts and alternate timing order between builds. XXH3 detachment sweeps
are available through `python_xxhash.py --thresholds --item-sizes 64 1024 --thresholds-kib 256 512 1024`, with
optional `--output PATH` for CSV output. Use the same `uv run` prefix for these diagnostics.

## Update the charts

1. Copy the measured GiB/s values into [results.csv](docs/benchmarks/results.csv). Update the series you measured.
   Retain unmeasured values and leave `gib_per_second` empty for unavailable results.
2. Keep matching input categories across implementations in each panel. Each row identifies a chart, panel,
   input category, and implementation. Row order controls chart, panel, category, and legend order.
3. Render the SVG files:

   ```sh
   uv run --python 3.15.0t --no-project python benchmarks/render_charts.py
   ```

4. Review the CSV and SVG diff. A focused update should change the corresponding charts, including any README
   overview that uses those values.

The renderer reads the CSV without running benchmarks or changing measurements. Keep temporary tuning sweeps,
branch comparisons, and profiler output out of this reference.
