"""Compare native Base64 batches against Python loops."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import gc
from collections.abc import Callable

import hashcodecs.base64 as hashcodecs_base64
import pybase64
from _support import data, pin_to_one_cpu, throughput

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


def run_matrix(item_sizes: tuple[int, ...], batch_sizes: tuple[int, ...]) -> None:
    for item_size in item_sizes:
        payloads = [data(item_size) for _ in range(max(batch_sizes))]
        encoded = [stdlib_base64.b64encode(payload) for payload in payloads]
        for batch_size in batch_sizes:
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
            encoded_outputs = [bytearray(len(value)) for value in encoded_batch]
            benchmark_into(
                'enc-in',
                item_size,
                batch_size,
                lambda batch=batch, outputs=encoded_outputs: hashcodecs_base64.b64encode_batch_into(batch, outputs),
                encoded_outputs,
                encoded_batch,
                (
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
                (
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


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--large', action='store_true', help='benchmark 1 MiB items with batches up to 32')
    arguments = parser.parse_args()

    pin_to_one_cpu()
    gc.disable()
    try:
        if arguments.large:
            run_matrix(LARGE_ITEM_SIZES, LARGE_BATCH_SIZES)
        else:
            run_matrix(ITEM_SIZES, BATCH_SIZES)
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
