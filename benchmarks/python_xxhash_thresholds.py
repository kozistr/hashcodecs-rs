"""Measure packed XXH3 latency around candidate batch detachment boundaries."""

from __future__ import annotations

import argparse
import csv
import gc
import sys
from pathlib import Path

from _support import add_timing_arguments, configure_timing, data, latency, pin_to_one_cpu, positive_int

import hashcodecs.xxhash as hashcodecs_xxhash


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--item-sizes', nargs='+', type=positive_int, default=[64, 1024, 65536])
    parser.add_argument('--thresholds-kib', nargs='+', type=positive_int, default=[256, 512, 1024])
    parser.add_argument('--output', type=Path)
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    configure_timing(arguments.samples, arguments.minimum_sample_seconds)
    pin_to_one_cpu()
    rows = []
    gc.disable()
    try:
        for size in arguments.item_sizes:
            counts = sorted(
                {
                    max(1, threshold * 1024 // size + delta)
                    for threshold in arguments.thresholds_kib
                    for delta in (-1, 0, 1)
                }
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
    finally:
        gc.enable()

    if arguments.output:
        with arguments.output.open('w', newline='') as destination:
            writer = csv.writer(destination)
            writer.writerow(('bits', 'item_bytes', 'items', 'total_bytes', 'latency_us'))
            writer.writerows(rows)
    print(sys.version)


if __name__ == '__main__':
    main()
