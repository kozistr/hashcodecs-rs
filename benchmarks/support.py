"""Shared helpers for pinned single-thread Python benchmarks."""

from __future__ import annotations

import argparse
import math
import threading
from collections.abc import Callable
from statistics import median
from time import perf_counter

from cpu import pin_to_one_cpu as pin_to_one_cpu

SIZES = (1024, 4 * 1024, 1024 * 1024, 8 * 1024 * 1024)
DEFAULT_SAMPLES = 15
DEFAULT_MINIMUM_SAMPLE_SECONDS = 0.2
SAMPLES = DEFAULT_SAMPLES
MINIMUM_SAMPLE_SECONDS = DEFAULT_MINIMUM_SAMPLE_SECONDS
_MISSING = object()


def positive_int(value: str) -> int:
    parsed = int(value)
    if parsed <= 0:
        raise argparse.ArgumentTypeError('must be positive')
    return parsed


def positive_float(value: str) -> float:
    parsed = float(value)
    if not math.isfinite(parsed) or parsed <= 0:
        raise argparse.ArgumentTypeError('must be a finite positive number')
    return parsed


def nonnegative_int(value: str) -> int:
    parsed = int(value)
    if parsed < 0:
        raise argparse.ArgumentTypeError('must be nonnegative')
    return parsed


def add_timing_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument('--quick', action='store_true', help='explore with 5 samples of 0.03 seconds; do not publish')
    parser.add_argument('--cpu', type=nonnegative_int, help='logical CPU to pin (default: first allowed CPU)')
    parser.add_argument(
        '--samples',
        type=positive_int,
        help=f'median sample count (default: {DEFAULT_SAMPLES})',
    )
    parser.add_argument(
        '--minimum-sample-seconds',
        type=positive_float,
        help=f'minimum duration of each sample (default: {DEFAULT_MINIMUM_SAMPLE_SECONDS})',
    )


def configure_timing(arguments: argparse.Namespace) -> None:
    global SAMPLES, MINIMUM_SAMPLE_SECONDS
    arguments.samples = arguments.samples or (5 if arguments.quick else DEFAULT_SAMPLES)
    arguments.minimum_sample_seconds = arguments.minimum_sample_seconds or (
        0.03 if arguments.quick else DEFAULT_MINIMUM_SAMPLE_SECONDS
    )
    SAMPLES = arguments.samples
    MINIMUM_SAMPLE_SECONDS = arguments.minimum_sample_seconds


def data(size: int) -> bytes:
    period = bytes((index * 31 + 17) & 0xFF for index in range(256))
    return period * (size // len(period)) + period[: size % len(period)]


def calibrate(function: Callable[[], object], minimum_seconds: float) -> int:
    iterations = 1
    while True:
        start = perf_counter()
        for _ in range(iterations):
            function()
        elapsed = perf_counter() - start
        if elapsed >= minimum_seconds:
            return iterations
        estimate = math.ceil(iterations * minimum_seconds / max(elapsed, 1e-9) * 1.05)
        iterations = max(iterations + 1, min(iterations * 10, estimate))


def throughput(function: Callable[[], object], input_size: int) -> float:
    nanoseconds = latency(function)
    return input_size * 1_000_000_000 / nanoseconds


def measure(
    operation: Callable[[], object],
    input_size: int,
    references: tuple[tuple[str, Callable[[], object]], ...] = (),
    *,
    hashcodecs_only: bool = False,
    expected: object = _MISSING,
) -> tuple[float, list[tuple[str, float]]]:
    result = operation()
    if expected is not _MISSING:
        assert result == expected
    for _, reference in references:
        assert result == reference()
    rate = throughput(operation, input_size)
    rates = [] if hashcodecs_only else [(label, throughput(reference, input_size)) for label, reference in references]
    return rate, rates


def format_rates(
    rate: float, references: list[tuple[str, float]], *, label: str = 'hashcodecs', item_size: int | None = None
) -> str:
    measurements = []
    for index, (name, value) in enumerate([(label, rate), *references]):
        text = f'{name}={value / 1024**3:6.2f} GiB/s'
        if item_size is not None:
            text += f' {value / item_size:10.0f} items/s'
        if index:
            text += f' ({rate / value:4.2f}x)'
        measurements.append(text)
    return '  '.join(measurements)


def latency(function: Callable[[], object]) -> float:
    """Return median nanoseconds per call after calibrating the sample size."""
    iterations = calibrate(function, MINIMUM_SAMPLE_SECONDS)

    samples = []
    for _ in range(SAMPLES):
        start = perf_counter()
        for _ in range(iterations):
            function()
        samples.append((perf_counter() - start) * 1_000_000_000 / iterations)
    return median(samples)


def threaded_throughput(function: Callable[[], object], input_size: int, workers: int) -> float:
    """Return aggregate bytes per second from equal work in Python threads."""
    iterations = calibrate(function, MINIMUM_SAMPLE_SECONDS)

    samples = []
    for _ in range(SAMPLES):
        barrier = threading.Barrier(workers + 1)
        errors: list[BaseException] = []

        def run(
            barrier: threading.Barrier = barrier,
            errors: list[BaseException] = errors,
        ) -> None:
            try:
                barrier.wait()
                for _ in range(iterations):
                    function()
            except BaseException as error:
                errors.append(error)

        threads = [threading.Thread(target=run) for _ in range(workers)]
        for thread in threads:
            thread.start()
        start = perf_counter()
        barrier.wait()
        for thread in threads:
            thread.join()
        if errors:
            raise errors[0]
        samples.append(input_size * iterations * workers / (perf_counter() - start))
    return median(samples)
