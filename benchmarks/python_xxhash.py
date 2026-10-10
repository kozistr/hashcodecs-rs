"""Compare Python XXH3 one-shot and batch APIs with upstream xxhash."""

from __future__ import annotations

import argparse
import csv
import gc
import sys
from collections.abc import Callable
from pathlib import Path

from _support import (
    SIZES,
    add_timing_arguments,
    configure_timing,
    data,
    format_rates,
    latency,
    measure,
    pin_to_one_cpu,
    positive_int,
    throughput,
)

import hashcodecs.xxhash as hashcodecs_xxhash
import xxhash


def report(
    name: str,
    input_size: int,
    ours: Callable[[], object],
    upstream: Callable[[], object],
    hashcodecs_only: bool,
) -> None:
    size = f'{input_size // 1024} KiB' if input_size % 1024 == 0 else f'{input_size} B'
    ours_rate, rates = measure(ours, input_size, (('xxhash', upstream),), hashcodecs_only=hashcodecs_only)
    print(f'{name:20} {size:>10}  {format_rates(ours_rate, rates)}')


def report_hashcodecs(name: str, input_size: int, operation: Callable[[], object]) -> None:
    rate = throughput(operation, input_size)
    print(f'{name:20} {input_size // 1024:>6} KiB  hashcodecs={rate / 1024**3:6.2f} GiB/s')


def benchmark_thresholds(item_sizes: list[int], thresholds_kib: list[int], csv_path: Path | None) -> None:
    rows = []
    for size in item_sizes:
        counts = sorted(
            {max(1, threshold * 1024 // size + delta) for threshold in thresholds_kib for delta in (-1, 0, 1)}
        )
        for count in counts:
            # Independent allocations with distinct contents avoid the repeated-object cache shortcut.
            items = [index.to_bytes(8, 'little')[:size] + data(max(0, size - 8)) for index in range(count)]
            for bits in (64, 128):
                one_shot = getattr(hashcodecs_xxhash, f'xxh3_{bits}')
                batch_into = getattr(hashcodecs_xxhash, f'xxh3_{bits}_batch_into')
                output = bytearray(count * bits // 8)
                assert batch_into(items, output, 42) == len(output)
                assert output == b''.join(one_shot(item, 42).to_bytes(bits // 8, 'little') for item in items)
                nanoseconds = latency(
                    lambda batch_into=batch_into, items=items, output=output: batch_into(items, output, 42)
                )
                rows.append((bits, size, count, size * count, f'{nanoseconds / 1000:.3f}'))
                print(f'XXH3-{bits} {size:6} bytes x {count:5}: {nanoseconds / 1000:9.3f} us', flush=True)
    if csv_path:
        with csv_path.open('w', newline='') as destination:
            writer = csv.writer(destination)
            writer.writerow(('bits', 'item_bytes', 'items', 'total_bytes', 'latency_us'))
            writer.writerows(rows)
    print(sys.version)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--hashcodecs-only',
        action='store_true',
        help='time hashcodecs without timing xxhash',
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        '--batches-only',
        action='store_true',
        help='time only allocating and packed batches',
    )
    mode.add_argument('--one-shot-only', action='store_true', help='time only single calls')
    parser.add_argument(
        '--sizes',
        nargs='+',
        type=positive_int,
        default=[16, 17, 33, 65, 97, 240, *SIZES],
        metavar='BYTES',
        help='one-shot input sizes in bytes',
    )
    parser.add_argument(
        '--batch-counts',
        nargs='+',
        type=positive_int,
        default=[32],
        metavar='COUNT',
        help='batch item counts to time (default: 32)',
    )
    mode.add_argument('--thresholds', action='store_true', help='measure packed XXH3 detachment boundaries')
    parser.add_argument('--item-sizes', nargs='+', type=positive_int)
    parser.add_argument('--thresholds-kib', nargs='+', type=positive_int)
    parser.add_argument('--output', type=Path, help='threshold CSV output')
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    if not arguments.thresholds and (arguments.item_sizes or arguments.thresholds_kib or arguments.output):
        parser.error('--item-sizes, --thresholds-kib, and --output require --thresholds')
    configure_timing(arguments)

    pin_to_one_cpu(arguments.cpu)
    gc.disable()
    try:
        if arguments.thresholds:
            benchmark_thresholds(
                arguments.item_sizes or [64, 1024, 65536],
                arguments.thresholds_kib or [256, 512, 1024],
                arguments.output,
            )
            return
        for size in () if arguments.batches_only else arguments.sizes:
            payload = data(size)
            report(
                'XXH3-64',
                size,
                lambda payload=payload: hashcodecs_xxhash.xxh3_64(payload, 42),
                lambda payload=payload: xxhash.xxh3_64_intdigest(payload, 42),
                arguments.hashcodecs_only,
            )
            report(
                'XXH3-128',
                size,
                lambda payload=payload: hashcodecs_xxhash.xxh3_128(payload, 42),
                lambda payload=payload: xxhash.xxh3_128_intdigest(payload, 42),
                arguments.hashcodecs_only,
            )

        for item_count in () if arguments.one_shot_only else arguments.batch_counts:
            print(f'\nBatch items: {item_count}')
            for size in (64, 1024, 4 * 1024, 1024 * 1024):
                items = [data(size) for _ in range(item_count)]
                total = size * len(items)

                output64 = bytearray(8 * len(items))
                expected64 = hashcodecs_xxhash.xxh3_64_batch(items, 42)
                assert hashcodecs_xxhash.xxh3_64_batch_into(items, output64, 42) == len(output64)
                assert output64 == b''.join(value.to_bytes(8, 'little') for value in expected64)
                report(
                    'XXH3-64 batch',
                    total,
                    lambda items=items: hashcodecs_xxhash.xxh3_64_batch(items, 42),
                    lambda items=items: [xxhash.xxh3_64_intdigest(item, 42) for item in items],
                    arguments.hashcodecs_only,
                )
                report_hashcodecs(
                    'XXH3-64 batch_into',
                    total,
                    lambda items=items, output=output64: hashcodecs_xxhash.xxh3_64_batch_into(items, output, 42),
                )

                output128 = bytearray(16 * len(items))
                expected128 = hashcodecs_xxhash.xxh3_128_batch(items, 42)
                assert hashcodecs_xxhash.xxh3_128_batch_into(items, output128, 42) == len(output128)
                assert output128 == b''.join(value.to_bytes(16, 'little') for value in expected128)
                report(
                    'XXH3-128 batch',
                    total,
                    lambda items=items: hashcodecs_xxhash.xxh3_128_batch(items, 42),
                    lambda items=items: [xxhash.xxh3_128_intdigest(item, 42) for item in items],
                    arguments.hashcodecs_only,
                )
                report_hashcodecs(
                    'XXH3-128 batch_into',
                    total,
                    lambda items=items, output=output128: hashcodecs_xxhash.xxh3_128_batch_into(items, output, 42),
                )

    finally:
        gc.enable()


if __name__ == '__main__':
    main()
