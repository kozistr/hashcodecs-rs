"""Compare Python XXH3 one-shot and batch APIs with upstream xxhash."""

from __future__ import annotations

import argparse
import gc
from collections.abc import Callable

import hashcodecs.xxhash as hashcodecs_xxhash
from _support import SIZES, data, pin_to_one_cpu, throughput

import xxhash


def report(
    name: str,
    input_size: int,
    ours: Callable[[], object],
    upstream: Callable[[], object],
    hashcodecs_only: bool,
) -> None:
    if not hashcodecs_only:
        assert ours() == upstream()
    ours_rate = throughput(ours, input_size)
    if hashcodecs_only:
        print(f'{name:14} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s')
        return
    upstream_rate = throughput(upstream, input_size)
    print(
        f'{name:14} {input_size // 1024:>6} KiB  '
        f'hashcodecs={ours_rate / 1024**3:6.2f} GiB/s  '
        f'xxhash={upstream_rate / 1024**3:6.2f} GiB/s  '
        f'({ours_rate / upstream_rate:4.2f}x)'
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        '--hashcodecs-only',
        action='store_true',
        help='time hashcodecs without timing xxhash',
    )
    arguments = parser.parse_args()

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

        for size in (64, 1024, 4 * 1024, 1024 * 1024):
            items = [data(size) for _ in range(32)]
            total = size * len(items)
            report(
                'XXH3-64 batch',
                total,
                lambda items=items: hashcodecs_xxhash.xxh3_64_batch(items, 42),
                lambda items=items: [xxhash.xxh3_64_intdigest(item, 42) for item in items],
                arguments.hashcodecs_only,
            )
            report(
                'XXH3-128 batch',
                total,
                lambda items=items: hashcodecs_xxhash.xxh3_128_batch(items, 42),
                lambda items=items: [xxhash.xxh3_128_intdigest(item, 42) for item in items],
                arguments.hashcodecs_only,
            )
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
