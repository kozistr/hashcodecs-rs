"""Measure Python call overhead and GIL-detachment boundaries."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import gc
from array import array
from collections.abc import Callable

import mmh3
from _support import (
    add_timing_arguments,
    configure_timing,
    data,
    latency,
    pin_to_one_cpu,
    threaded_throughput,
)

import hashcodecs.base64 as hashcodecs_base64
import hashcodecs.murmur3 as hashcodecs_murmur3
import hashcodecs.xxhash as hashcodecs_xxhash
import xxhash

SMALL_SIZES = (0, 1, 8, 16, 32, 64, 128, 240, 256)
THRESHOLD_SIZES = tuple(size * 1024 for size in (16, 32, 48, 64, 96, 128, 192, 256, 384, 512))
THREAD_SIZES = tuple(size * 1024 for size in (16, 64, 256, 1024))
BUFFER_SIZES = (64, 4096)
Case = tuple[str, int, Callable[[], object], object]


def cases(size: int, *, keywords: bool = False) -> tuple[Case, ...]:
    payload = data(size)
    encoded = stdlib_base64.b64encode(payload)
    expected_x86_32 = mmh3.mmh3_32_uintdigest(payload, 0)
    expected_x86_128 = mmh3.mmh3_x86_128_digest(payload, 0)
    expected_x64_128 = mmh3.mmh3_x64_128_digest(payload, 0)
    expected_xxh3_64 = xxhash.xxh3_64_intdigest(payload, 0)
    expected_xxh3_128 = xxhash.xxh3_128_intdigest(payload, 0)
    if keywords:
        return (
            ('Base64 encode', len(payload), lambda: hashcodecs_base64.standard_b64encode(s=payload), encoded),
            ('Base64 decode', len(encoded), lambda: hashcodecs_base64.b64decode(s=encoded, validate=True), payload),
            (
                'Murmur x86-32',
                len(payload),
                lambda: hashcodecs_murmur3.murmur3_32(s=payload, seed=0),
                expected_x86_32,
            ),
            (
                'Murmur x86-128',
                len(payload),
                lambda: hashcodecs_murmur3.murmur3_x86_128_digest(s=payload, seed=0),
                expected_x86_128,
            ),
            (
                'Murmur x64-128',
                len(payload),
                lambda: hashcodecs_murmur3.murmur3_x64_128_digest(s=payload, seed=0),
                expected_x64_128,
            ),
            ('XXH3-64', len(payload), lambda: hashcodecs_xxhash.xxh3_64(s=payload, seed=0), expected_xxh3_64),
            (
                'XXH3-128',
                len(payload),
                lambda: hashcodecs_xxhash.xxh3_128(s=payload, seed=0),
                expected_xxh3_128,
            ),
        )
    return (
        ('Base64 encode', len(payload), lambda: hashcodecs_base64.standard_b64encode(payload), encoded),
        ('Base64 decode', len(encoded), lambda: hashcodecs_base64.b64decode(encoded, validate=True), payload),
        ('Murmur x86-32', len(payload), lambda: hashcodecs_murmur3.murmur3_32(payload, 0), expected_x86_32),
        (
            'Murmur x86-128',
            len(payload),
            lambda: hashcodecs_murmur3.murmur3_x86_128_digest(payload, 0),
            expected_x86_128,
        ),
        (
            'Murmur x64-128',
            len(payload),
            lambda: hashcodecs_murmur3.murmur3_x64_128_digest(payload, 0),
            expected_x64_128,
        ),
        ('XXH3-64', len(payload), lambda: hashcodecs_xxhash.xxh3_64(payload, 0), expected_xxh3_64),
        ('XXH3-128', len(payload), lambda: hashcodecs_xxhash.xxh3_128(payload, 0), expected_xxh3_128),
    )


def report(name: str, size: int, operation: Callable[[], object], expected: object, call_shape: str) -> None:
    assert operation() == expected
    nanoseconds = latency(operation)
    size_label = f'{size} B' if size < 1024 else f'{size // 1024} KiB'
    rate = size / nanoseconds if nanoseconds else 0.0
    print(f'{name:15} {call_shape:10} {size_label:>8} {nanoseconds:10.1f} ns/call {rate:7.2f} GB/s')


def report_buffer(name: str, payload: bytes, value: object, expected: int) -> None:
    def operation() -> int:
        return hashcodecs_xxhash.xxh3_64(value)

    assert operation() == expected
    print(f'{len(payload):>8} B  {name:20} {latency(operation):9.2f} ns/call')


def buffer_inputs(payload: bytes) -> tuple[tuple[str, object], ...]:
    padded = b'\xa5' + payload + b'\x5a'
    interleaved = bytearray(len(payload) * 2)
    interleaved[::2] = payload
    return (
        ('bytes', payload),
        ('memoryview', memoryview(payload)),
        ('sliced memoryview', memoryview(padded)[1:-1]),
        ('bytearray view', memoryview(bytearray(payload))),
        ('noncontiguous view', memoryview(interleaved)[::2]),
        ("array('B')", array('B', payload)),
    )


def report_threads(name: str, size: int, operation: Callable[[], object], expected: object) -> None:
    assert operation() == expected
    baseline = threaded_throughput(operation, size, 1)
    for workers in (1, 2, 4):
        rate = baseline if workers == 1 else threaded_throughput(operation, size, workers)
        suffix = '' if workers == 1 else 's'
        print(
            f'{name:15} {workers} thread{suffix:1} '
            f'{size // 1024:>6} KiB {rate / 1024**3:7.2f} GiB/s {rate / baseline:5.2f}x'
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        '--thresholds',
        action='store_true',
        help='measure calls around candidate GIL-detachment thresholds',
    )
    mode.add_argument(
        '--keywords',
        action='store_true',
        help='compare positional and keyword calls at 64 bytes',
    )
    mode.add_argument(
        '--thread-scaling',
        action='store_true',
        help='measure aggregate throughput with one, two, and four threads',
    )
    mode.add_argument(
        '--buffer-inputs',
        action='store_true',
        help='compare XXH3-64 call costs across bytes-like input types',
    )
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    configure_timing(arguments.samples, arguments.minimum_sample_seconds)

    if not arguments.thread_scaling:
        pin_to_one_cpu()
    gc.disable()
    try:
        if arguments.buffer_inputs:
            for requested_size in BUFFER_SIZES:
                payload = data(requested_size)
                expected = hashcodecs_xxhash.xxh3_64(payload)
                for name, value in buffer_inputs(payload):
                    report_buffer(name, payload, value, expected)
            return

        if arguments.keywords:
            for call_shape, keyword_arguments in (('positional', False), ('keyword', True)):
                for name, size, operation, expected in cases(64, keywords=keyword_arguments):
                    report(name, size, operation, expected, call_shape)
            return

        if arguments.thread_scaling:
            selected = {'Base64 encode', 'Murmur x64-128', 'XXH3-64'}
            for requested_size in THREAD_SIZES:
                for name, size, operation, expected in cases(requested_size):
                    if name in selected:
                        report_threads(name, size, operation, expected)
            return

        sizes = THRESHOLD_SIZES if arguments.thresholds else SMALL_SIZES
        call_shape = 'threshold' if arguments.thresholds else 'positional'
        for requested_size in sizes:
            for name, size, operation, expected in cases(requested_size):
                report(name, size, operation, expected, call_shape)
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
