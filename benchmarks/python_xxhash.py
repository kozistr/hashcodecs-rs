"""Compare Python XXH3 one-shot and batch APIs with upstream xxhash."""

from __future__ import annotations

import argparse
import gc
from collections.abc import Callable

from _support import SIZES, add_timing_arguments, configure_timing, data, pin_to_one_cpu, positive_int, throughput

import hashcodecs.xxhash as hashcodecs_xxhash
import xxhash


def report(
    name: str,
    input_size: int,
    ours: Callable[[], object],
    upstream: Callable[[], object],
    hashcodecs_only: bool,
) -> None:
    assert ours() == upstream()
    ours_rate = throughput(ours, input_size)
    if hashcodecs_only:
        print(f'{name:20} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s')
        return
    upstream_rate = throughput(upstream, input_size)
    print(
        f'{name:20} {input_size // 1024:>6} KiB  '
        f'hashcodecs={ours_rate / 1024**3:6.2f} GiB/s  '
        f'xxhash={upstream_rate / 1024**3:6.2f} GiB/s  '
        f'({ours_rate / upstream_rate:4.2f}x)'
    )


def report_hashcodecs(name: str, input_size: int, operation: Callable[[], object]) -> None:
    rate = throughput(operation, input_size)
    print(f'{name:20} {input_size // 1024:>6} KiB  hashcodecs={rate / 1024**3:6.2f} GiB/s')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--hashcodecs-only',
        action='store_true',
        help='time hashcodecs without timing xxhash',
    )
    parser.add_argument(
        '--batch-counts',
        nargs='+',
        type=positive_int,
        default=[32],
        metavar='COUNT',
        help='batch item counts to time (default: 32)',
    )
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    configure_timing(arguments.samples, arguments.minimum_sample_seconds)

    pin_to_one_cpu()
    gc.disable()
    try:
        for size in SIZES:
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

        for item_count in arguments.batch_counts:
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
