"""Shared helpers for pinned single-thread Python benchmarks."""

from __future__ import annotations

import ctypes
import os
import sys
from collections.abc import Callable
from statistics import median
from time import perf_counter

SIZES = (4 * 1024, 1024 * 1024, 32 * 1024 * 1024)
SAMPLES = 15
MINIMUM_SAMPLE_SECONDS = 0.2


def pin_to_one_cpu() -> None:
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
        first_available_cpu = process_mask.value & -process_mask.value
        if set_process_affinity(process, first_available_cpu) == 0:
            raise ctypes.WinError(ctypes.get_last_error())
        return

    get_affinity = getattr(os, 'sched_getaffinity', None)
    set_affinity = getattr(os, 'sched_setaffinity', None)
    if get_affinity is not None and set_affinity is not None:
        available = get_affinity(0)
        set_affinity(0, {min(available)})


def data(size: int) -> bytes:
    period = bytes((index * 31 + 17) & 0xFF for index in range(256))
    return period * (size // len(period)) + period[: size % len(period)]


def throughput(function: Callable[[], object], input_size: int) -> float:
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
