"""Compare the Python Base64 API against CPython and pybase64."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import ctypes
import gc
import os
import sys
from collections.abc import Callable
from statistics import median
from time import perf_counter

import hashcodecs.base64 as hashcodecs_base64
import pybase64

SIZES = (4 * 1024, 1024 * 1024, 32 * 1024 * 1024)
SAMPLES = 15
MINIMUM_SAMPLE_SECONDS = 0.2


def pin_to_one_cpu() -> None:
    if sys.platform == "win32":
        kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
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
        first_available_cpu = process_mask.value & -process_mask.value
        if set_process_affinity(process, first_available_cpu) == 0:
            raise ctypes.WinError(ctypes.get_last_error())
        return

    get_affinity = getattr(os, "sched_getaffinity", None)
    set_affinity = getattr(os, "sched_setaffinity", None)
    if get_affinity is not None and set_affinity is not None:
        available = get_affinity(0)
        set_affinity(0, {min(available)})


def data(size: int) -> bytes:
    period = bytes((index * 31 + 17) & 0xFF for index in range(256))
    return period * (size // len(period)) + period[: size % len(period)]


def throughput(function: Callable[[], bytes], input_size: int) -> float:
    iterations = 1
    while True:
        start = perf_counter()
        for _ in range(iterations):
            function()
        elapsed = perf_counter() - start
        if elapsed >= MINIMUM_SAMPLE_SECONDS:
            break
        iterations *= 2

    samples = []
    for _ in range(SAMPLES):
        start = perf_counter()
        for _ in range(iterations):
            function()
        samples.append(input_size * iterations / (perf_counter() - start))
    return median(samples)


def benchmark(
    name: str,
    input_size: int,
    ours: Callable[[], bytes],
    references: tuple[tuple[str, Callable[[], bytes]], ...],
) -> None:
    ours_result = ours()
    for _, reference in references:
        assert ours_result == reference()
    ours_rate = throughput(ours, input_size)
    measurements = [f"hashcodecs={ours_rate / 1024**3:6.2f} GiB/s"]
    for label, reference in references:
        reference_rate = throughput(reference, input_size)
        measurements.append(f"{label}={reference_rate / 1024**3:6.2f} GiB/s ({ours_rate / reference_rate:4.2f}x)")
    print(f"{name:18} {input_size // 1024:>6} KiB  {'  '.join(measurements)}")


def benchmark_ours(name: str, input_size: int, ours: Callable[[], bytes], expected: bytes) -> None:
    assert ours() == expected
    ours_rate = throughput(ours, input_size)
    print(f"{name:22} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--hashcodecs-only",
        action="store_true",
        help="time hashcodecs without timing competitors",
    )
    args = parser.parse_args()

    pin_to_one_cpu()
    gc.disable()
    try:
        for size in SIZES:
            payload = data(size)
            standard = stdlib_base64.b64encode(payload)
            urlsafe = stdlib_base64.urlsafe_b64encode(payload)

            if args.hashcodecs_only:
                benchmark_ours(
                    "standard encode",
                    size,
                    lambda payload=payload: hashcodecs_base64.standard_b64encode(payload),
                    standard,
                )
                benchmark_ours(
                    "standard decode",
                    size,
                    lambda standard=standard: hashcodecs_base64.b64decode(standard, validate=True),
                    payload,
                )
                benchmark_ours(
                    "URL-safe encode",
                    size,
                    lambda payload=payload: hashcodecs_base64.urlsafe_b64encode(payload),
                    urlsafe,
                )
                benchmark_ours(
                    "URL-safe decode",
                    size,
                    lambda urlsafe=urlsafe: hashcodecs_base64.b64decode(urlsafe, b"-_", validate=True),
                    payload,
                )
                continue

            benchmark(
                "standard encode",
                size,
                lambda payload=payload: hashcodecs_base64.standard_b64encode(payload),
                (
                    ("stdlib", lambda payload=payload: stdlib_base64.b64encode(payload)),
                    ("pybase64", lambda payload=payload: pybase64.standard_b64encode(payload)),
                ),
            )
            benchmark(
                "standard decode",
                size,
                lambda standard=standard: hashcodecs_base64.b64decode(standard, validate=True),
                (
                    (
                        "stdlib",
                        lambda standard=standard: stdlib_base64.b64decode(standard, validate=True),
                    ),
                    (
                        "pybase64",
                        lambda standard=standard: pybase64.b64decode(standard, validate=True),
                    ),
                ),
            )
            benchmark(
                "URL-safe encode",
                size,
                lambda payload=payload: hashcodecs_base64.urlsafe_b64encode(payload),
                (
                    ("stdlib", lambda payload=payload: stdlib_base64.urlsafe_b64encode(payload)),
                    ("pybase64", lambda payload=payload: pybase64.urlsafe_b64encode(payload)),
                ),
            )
            benchmark(
                "URL-safe decode",
                size,
                lambda urlsafe=urlsafe: hashcodecs_base64.b64decode(urlsafe, b"-_", validate=True),
                (
                    (
                        "stdlib",
                        lambda urlsafe=urlsafe: stdlib_base64.b64decode(urlsafe, b"-_", validate=True),
                    ),
                    (
                        "pybase64",
                        lambda urlsafe=urlsafe: pybase64.b64decode(urlsafe, b"-_", validate=True),
                    ),
                ),
            )
    finally:
        gc.enable()


if __name__ == "__main__":
    main()
