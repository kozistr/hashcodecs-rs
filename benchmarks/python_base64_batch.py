"""Compare native Base64 batches against Python loops."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import gc
import sys
import time
import tracemalloc
from collections.abc import Callable

import pybase64
from _support import (
    add_timing_arguments,
    configure_timing,
    data,
    pin_to_one_cpu,
    positive_float,
    positive_int,
    throughput,
)

import hashcodecs.base64 as hashcodecs_base64

BATCH_SIZES = (8, 64, 1024)
ITEM_SIZES = (16, 256, 4096)
LARGE_BATCH_SIZES = (1, 2, 4, 8, 16, 32)
LARGE_ITEM_SIZES = (1024 * 1024,)


def benchmark(
    name: str,
    item_size: int,
    batch_size: int,
    ours: Callable[[], list[bytes]],
    references: tuple[tuple[str, Callable[[], list[bytes]]], ...],
) -> None:
    ours_result = ours()
    for _, reference in references:
        assert ours_result == reference()

    total_size = item_size * batch_size
    ours_rate = throughput(ours, total_size)
    measurements = [f'hashcodecs={ours_rate / 1024**3:6.2f} GiB/s {ours_rate / item_size:10.0f} items/s']
    for label, reference in references:
        reference_rate = throughput(reference, total_size)
        measurements.append(
            f'{label}={reference_rate / 1024**3:6.2f} GiB/s '
            f'{reference_rate / item_size:10.0f} items/s ({ours_rate / reference_rate:4.2f}x)'
        )
    print(f'{name:6} item={item_size:4} B  batch={batch_size:4}  {"  ".join(measurements)}')


def benchmark_into(
    name: str,
    item_size: int,
    batch_size: int,
    ours: Callable[[], list[int]],
    outputs: list[bytearray],
    expected: list[bytes],
    references: tuple[tuple[str, Callable[[], object]], ...],
) -> None:
    written = ours()
    assert [bytes(output[:length]) for output, length in zip(outputs, written, strict=True)] == expected

    total_size = item_size * batch_size
    ours_rate = throughput(ours, total_size)
    measurements = [f'batch-into={ours_rate / 1024**3:6.2f} GiB/s {ours_rate / item_size:10.0f} items/s']
    for label, reference in references:
        reference_rate = throughput(reference, total_size)
        measurements.append(
            f'{label}={reference_rate / 1024**3:6.2f} GiB/s '
            f'{reference_rate / item_size:10.0f} items/s ({ours_rate / reference_rate:4.2f}x)'
        )
    print(f'{name:6} item={item_size:7} B  batch={batch_size:4}  {"  ".join(measurements)}')


def run_matrix(
    item_sizes: tuple[int, ...],
    batch_sizes: tuple[int, ...],
    hashcodecs_only: bool,
    decode_only: bool,
) -> None:
    for item_size in item_sizes:
        payloads = [data(item_size) for _ in range(max(batch_sizes))]
        encoded = [stdlib_base64.b64encode(payload) for payload in payloads]
        for batch_size in batch_sizes:
            batch = payloads[:batch_size]
            encoded_batch = encoded[:batch_size]
            if not decode_only:
                benchmark(
                    'encode',
                    item_size,
                    batch_size,
                    lambda batch=batch: hashcodecs_base64.b64encode_batch(batch),
                    ()
                    if hashcodecs_only
                    else (
                        ('hash-loop', lambda batch=batch: [hashcodecs_base64.b64encode(item) for item in batch]),
                        ('pybase64', lambda batch=batch: [pybase64.b64encode(item) for item in batch]),
                        ('stdlib', lambda batch=batch: [stdlib_base64.b64encode(item) for item in batch]),
                    ),
                )
                encoded_outputs = [bytearray(len(value)) for value in encoded_batch]
                benchmark_into(
                    'enc-in',
                    item_size,
                    batch_size,
                    lambda batch=batch, outputs=encoded_outputs: hashcodecs_base64.b64encode_batch_into(
                        batch, outputs
                    ),
                    encoded_outputs,
                    encoded_batch,
                    ()
                    if hashcodecs_only
                    else (
                        (
                            'into-loop',
                            lambda batch=batch, outputs=encoded_outputs: [
                                hashcodecs_base64.b64encode_into(item, output)
                                for item, output in zip(batch, outputs, strict=True)
                            ],
                        ),
                        ('returned', lambda batch=batch: hashcodecs_base64.b64encode_batch(batch)),
                    ),
                )
            benchmark(
                'decode',
                item_size,
                batch_size,
                lambda encoded_batch=encoded_batch: hashcodecs_base64.b64decode_batch(encoded_batch, validate=True),
                ()
                if hashcodecs_only
                else (
                    (
                        'hash-loop',
                        lambda encoded_batch=encoded_batch: [
                            hashcodecs_base64.b64decode(item, validate=True) for item in encoded_batch
                        ],
                    ),
                    (
                        'pybase64',
                        lambda encoded_batch=encoded_batch: [
                            pybase64.b64decode(item, validate=True) for item in encoded_batch
                        ],
                    ),
                    (
                        'stdlib',
                        lambda encoded_batch=encoded_batch: [
                            stdlib_base64.b64decode(item, validate=True) for item in encoded_batch
                        ],
                    ),
                ),
            )
            decoded_outputs = [bytearray(item_size) for _ in encoded_batch]
            benchmark_into(
                'dec-in',
                item_size,
                batch_size,
                lambda inputs=encoded_batch, outputs=decoded_outputs: hashcodecs_base64.b64decode_batch_into(
                    inputs, outputs, validate=True
                ),
                decoded_outputs,
                batch,
                ()
                if hashcodecs_only
                else (
                    (
                        'into-loop',
                        lambda inputs=encoded_batch, outputs=decoded_outputs: [
                            hashcodecs_base64.b64decode_into(item, output, validate=True)
                            for item, output in zip(inputs, outputs, strict=True)
                        ],
                    ),
                    (
                        'returned',
                        lambda inputs=encoded_batch: hashcodecs_base64.b64decode_batch(inputs, validate=True),
                    ),
                ),
            )


def encode_operations(
    item_size: int, batch_size: int
) -> tuple[dict[str, Callable[[], object]], list[bytes], list[bytearray]]:
    payloads = [data(item_size) for _ in range(batch_size)]
    encoded = [stdlib_base64.b64encode(payload) for payload in payloads]
    outputs = [bytearray(len(value)) for value in encoded]
    operations: dict[str, Callable[[], object]] = {
        'returned': lambda: hashcodecs_base64.b64encode_batch(payloads),
        'batch-into': lambda: hashcodecs_base64.b64encode_batch_into(payloads, outputs),
        'hash-loop': lambda: [hashcodecs_base64.b64encode(item) for item in payloads],
        'pybase64-loop': lambda: [pybase64.b64encode(item) for item in payloads],
        'stdlib-loop': lambda: [stdlib_base64.b64encode(item) for item in payloads],
    }
    return operations, encoded, outputs


def decode_operations(
    item_size: int, batch_size: int
) -> tuple[dict[str, Callable[[], object]], list[bytes], list[bytearray]]:
    payloads = [data(item_size) for _ in range(batch_size)]
    encoded = [stdlib_base64.b64encode(payload) for payload in payloads]
    outputs = [bytearray(item_size) for _ in encoded]
    operations: dict[str, Callable[[], object]] = {
        'returned': lambda: hashcodecs_base64.b64decode_batch(encoded, validate=True),
        'batch-into': lambda: hashcodecs_base64.b64decode_batch_into(encoded, outputs, validate=True),
        'hash-loop': lambda: [hashcodecs_base64.b64decode(item, validate=True) for item in encoded],
        'pybase64-loop': lambda: [pybase64.b64decode(item, validate=True) for item in encoded],
        'stdlib-loop': lambda: [stdlib_base64.b64decode(item, validate=True) for item in encoded],
    }
    return operations, payloads, outputs


def check_result(name: str, result: object, expected: list[bytes], outputs: list[bytearray]) -> None:
    if name == 'batch-into':
        assert isinstance(result, list)
        assert [bytes(output[:length]) for output, length in zip(outputs, result, strict=True)] == expected
    else:
        assert result == expected


def profile_operation(
    direction: str,
    name: str,
    item_size: int,
    batch_size: int,
    seconds: float,
    discard_result: bool,
) -> None:
    operation_factory = encode_operations if direction == 'encode' else decode_operations
    operations, expected, outputs = operation_factory(item_size, batch_size)
    operation = operations[name]
    result = operation()
    check_result(name, result, expected, outputs)
    if discard_result:
        del result
    iterations = 0
    started = time.perf_counter()
    deadline = started + seconds
    while time.perf_counter() < deadline:
        for _ in range(8):
            if discard_result:
                operation()
            else:
                result = operation()
            iterations += 1
    elapsed = time.perf_counter() - started
    if discard_result:
        result = operation()
    check_result(name, result, expected, outputs)
    rate = item_size * batch_size * iterations / elapsed
    lifetime = 'discarded' if discard_result else 'retained'
    print(
        f'profile direction={direction} operation={name} item={item_size} B batch={batch_size} '
        f'lifetime={lifetime} iterations={iterations} '
        f'elapsed={elapsed:.3f} s throughput={rate / 1024**3:.2f} GiB/s'
    )


def allocation_profile(direction: str, item_size: int, batch_size: int) -> None:
    operation_factory = encode_operations if direction == 'encode' else decode_operations
    operations, expected, outputs = operation_factory(item_size, batch_size)
    tracemalloc.start()
    try:
        for name in ('returned', 'batch-into', 'hash-loop'):
            gc.collect()
            tracemalloc.clear_traces()
            before, _ = tracemalloc.get_traced_memory()
            result = operations[name]()
            current, peak = tracemalloc.get_traced_memory()
            check_result(name, result, expected, outputs)
            retained = sys.getsizeof(result) + sum(sys.getsizeof(item) for item in result)
            print(
                f'alloc direction={direction} operation={name:10} item={item_size} B batch={batch_size} '
                f'traced-retained={current - before:,} B traced-peak={peak - before:,} B '
                f'result-size={retained:,} B'
            )
    finally:
        tracemalloc.stop()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--large', action='store_true', help='benchmark 1 MiB items with batches up to 32')
    parser.add_argument('--item-sizes', nargs='+', type=positive_int, help='override item sizes in bytes')
    parser.add_argument('--batch-sizes', nargs='+', type=positive_int, help='override batch item counts')
    parser.add_argument('--decode-only', action='store_true', help='skip encode benchmarks')
    parser.add_argument(
        '--hashcodecs-only',
        action='store_true',
        help='time hashcodecs without timing reference implementations',
    )
    profile_mode = parser.add_mutually_exclusive_group()
    profile_mode.add_argument(
        '--profile-operation',
        choices=('returned', 'batch-into', 'hash-loop', 'pybase64-loop', 'stdlib-loop'),
        help='run one decode operation continuously for an external sampling profiler',
    )
    parser.add_argument(
        '--profile-seconds',
        type=positive_float,
        default=10.0,
        help='duration for --profile-operation (default: 10)',
    )
    parser.add_argument(
        '--discard-profile-result',
        action='store_true',
        help='discard each profiler result before starting the next operation',
    )
    profile_mode.add_argument(
        '--allocation-profile', action='store_true', help='measure retained and peak operation allocations'
    )
    parser.add_argument(
        '--profile-direction',
        choices=('encode', 'decode'),
        default='decode',
        help='operation direction for profiling modes (default: decode)',
    )
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    if arguments.large and (arguments.item_sizes or arguments.batch_sizes):
        parser.error('--large cannot be combined with --item-sizes or --batch-sizes')
    item_sizes = tuple(arguments.item_sizes or (LARGE_ITEM_SIZES if arguments.large else ITEM_SIZES))
    batch_sizes = tuple(arguments.batch_sizes or (LARGE_BATCH_SIZES if arguments.large else BATCH_SIZES))
    if (arguments.profile_operation or arguments.allocation_profile) and (
        len(item_sizes) != 1 or len(batch_sizes) != 1
    ):
        parser.error('profiling requires exactly one --item-sizes value and one --batch-sizes value')
    if arguments.discard_profile_result and not arguments.profile_operation:
        parser.error('--discard-profile-result requires --profile-operation')
    configure_timing(arguments.samples, arguments.minimum_sample_seconds)

    pin_to_one_cpu()
    gc.disable()
    try:
        if arguments.profile_operation:
            profile_operation(
                arguments.profile_direction,
                arguments.profile_operation,
                item_sizes[0],
                batch_sizes[0],
                arguments.profile_seconds,
                arguments.discard_profile_result,
            )
        elif arguments.allocation_profile:
            allocation_profile(arguments.profile_direction, item_sizes[0], batch_sizes[0])
        else:
            run_matrix(item_sizes, batch_sizes, arguments.hashcodecs_only, arguments.decode_only)
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
