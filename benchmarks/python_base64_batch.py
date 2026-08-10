"""Compare native Base64 batches against Python loops."""

from __future__ import annotations

import base64 as stdlib_base64
import gc
from collections.abc import Callable

import hashcodecs.base64 as hashcodecs_base64
import pybase64
from _support import data, pin_to_one_cpu, throughput

BATCH_SIZES = (8, 64, 1024)
ITEM_SIZES = (16, 256, 4096)


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


def main() -> None:
    pin_to_one_cpu()
    gc.disable()
    try:
        for item_size in ITEM_SIZES:
            payloads = [data(item_size) for _ in range(max(BATCH_SIZES))]
            encoded = [stdlib_base64.b64encode(payload) for payload in payloads]
            for batch_size in BATCH_SIZES:
                batch = payloads[:batch_size]
                encoded_batch = encoded[:batch_size]
                benchmark(
                    'encode',
                    item_size,
                    batch_size,
                    lambda batch=batch: hashcodecs_base64.b64encode_batch(batch),
                    (
                        ('hash-loop', lambda batch=batch: [hashcodecs_base64.b64encode(item) for item in batch]),
                        ('pybase64', lambda batch=batch: [pybase64.b64encode(item) for item in batch]),
                        ('stdlib', lambda batch=batch: [stdlib_base64.b64encode(item) for item in batch]),
                    ),
                )
                benchmark(
                    'decode',
                    item_size,
                    batch_size,
                    lambda encoded_batch=encoded_batch: hashcodecs_base64.b64decode_batch(
                        encoded_batch, validate=True
                    ),
                    (
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
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
