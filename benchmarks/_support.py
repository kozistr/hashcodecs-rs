"""Shared helpers for pinned single-thread Python benchmarks."""

from __future__ import annotations

import argparse
import ctypes
import math
import os
import sys
import threading
from collections.abc import Callable
from statistics import median
from time import perf_counter

SIZES = (1024, 4 * 1024, 1024 * 1024, 8 * 1024 * 1024)
DEFAULT_SAMPLES = 15
DEFAULT_MINIMUM_SAMPLE_SECONDS = 0.2
SAMPLES = DEFAULT_SAMPLES
MINIMUM_SAMPLE_SECONDS = DEFAULT_MINIMUM_SAMPLE_SECONDS


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


def pin_to_one_cpu(cpu: int | None = None) -> None:
    if sys.platform == 'win32':
        kernel32 = ctypes.WinDLL('kernel32', use_last_error=True)
        get_current_process = kernel32.GetCurrentProcess
        get_current_process.restype = ctypes.c_void_p
        get_process_affinity = kernel32.GetProcessAffinityMask
        get_process_affinity.argtypes = (
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_size_t),
            ctypes.POINTER(ctypes.c_size_t),
        )
        get_process_affinity.restype = ctypes.c_int
        set_process_affinity = kernel32.SetProcessAffinityMask
        set_process_affinity.argtypes = (ctypes.c_void_p, ctypes.c_size_t)
        set_process_affinity.restype = ctypes.c_int
        process = get_current_process()
        process_mask = ctypes.c_size_t()
        system_mask = ctypes.c_size_t()
        if get_process_affinity(process, ctypes.byref(process_mask), ctypes.byref(system_mask)) == 0:
            raise ctypes.WinError(ctypes.get_last_error())
        selected = process_mask.value & -process_mask.value if cpu is None else 1 << cpu
        if selected & process_mask.value != selected:
            raise ValueError(f'CPU {cpu} is outside the process affinity mask')
        if set_process_affinity(process, selected) == 0:
            raise ctypes.WinError(ctypes.get_last_error())
        return

    get_affinity = getattr(os, 'sched_getaffinity', None)
    set_affinity = getattr(os, 'sched_setaffinity', None)
    if get_affinity is not None and set_affinity is not None:
        available = get_affinity(0)
        selected = min(available) if cpu is None else cpu
        if selected not in available:
            raise ValueError(f'CPU {selected} is outside the process affinity mask')
        set_affinity(0, {selected})
    elif cpu is not None:
        raise RuntimeError('explicit CPU affinity is unavailable on this platform')


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
