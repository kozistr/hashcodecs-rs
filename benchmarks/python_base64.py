"""Compare the Python Base64 API against CPython and pybase64."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import gc
from collections.abc import Callable

import hashcodecs.base64 as hashcodecs_base64
import pybase64
from _support import SIZES, data, pin_to_one_cpu, throughput


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
    measurements = [f'hashcodecs={ours_rate / 1024**3:6.2f} GiB/s']
    for label, reference in references:
        reference_rate = throughput(reference, input_size)
        measurements.append(f'{label}={reference_rate / 1024**3:6.2f} GiB/s ({ours_rate / reference_rate:4.2f}x)')
    print(f'{name:18} {input_size // 1024:>6} KiB  {"  ".join(measurements)}')


def benchmark_ours(name: str, input_size: int, ours: Callable[[], bytes], expected: bytes) -> None:
    assert ours() == expected
    ours_rate = throughput(ours, input_size)
    print(f'{name:22} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s')


def benchmark_into(
    name: str,
    input_size: int,
    operation: Callable[[], int],
    output: bytearray,
    expected: bytes,
) -> None:
    written = operation()
    assert output[:written] == expected
    ours_rate = throughput(operation, input_size)
    print(f'{name:22} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s')


def main() -> None:
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        '--hashcodecs-only',
        action='store_true',
        help='time hashcodecs without timing competitors',
    )
    mode.add_argument(
        '--into',
        action='store_true',
        help='time hashcodecs with reusable bytearray output buffers',
    )
    args = parser.parse_args()

    pin_to_one_cpu()
    gc.disable()
    try:
        for size in SIZES:
            payload = data(size)
            standard = stdlib_base64.b64encode(payload)
            urlsafe = stdlib_base64.urlsafe_b64encode(payload)

            if args.into:
                encoded_output = bytearray(len(standard))
                decoded_output = bytearray(size)
                benchmark_into(
                    'standard encode into',
                    size,
                    lambda payload=payload, output=encoded_output: hashcodecs_base64.b64encode_into(payload, output),
                    encoded_output,
                    standard,
                )
                benchmark_into(
                    'standard decode into',
                    size,
                    lambda standard=standard, output=decoded_output: hashcodecs_base64.b64decode_into(
                        standard, output, validate=True
                    ),
                    decoded_output,
                    payload,
                )
                benchmark_into(
                    'URL-safe encode into',
                    size,
                    lambda payload=payload, output=encoded_output: hashcodecs_base64.b64encode_into(
                        payload, output, b'-_'
                    ),
                    encoded_output,
                    urlsafe,
                )
                benchmark_into(
                    'URL-safe decode into',
                    size,
                    lambda urlsafe=urlsafe, output=decoded_output: hashcodecs_base64.b64decode_into(
                        urlsafe, output, b'-_', validate=True
                    ),
                    decoded_output,
                    payload,
                )
                continue

            if args.hashcodecs_only:
                benchmark_ours(
                    'standard encode',
                    size,
                    lambda payload=payload: hashcodecs_base64.standard_b64encode(payload),
                    standard,
                )
                benchmark_ours(
                    'standard decode',
                    size,
                    lambda standard=standard: hashcodecs_base64.b64decode(standard, validate=True),
                    payload,
                )
                benchmark_ours(
                    'URL-safe encode',
                    size,
                    lambda payload=payload: hashcodecs_base64.urlsafe_b64encode(payload),
                    urlsafe,
                )
                benchmark_ours(
                    'URL-safe decode',
                    size,
                    lambda urlsafe=urlsafe: hashcodecs_base64.b64decode(urlsafe, b'-_', validate=True),
                    payload,
                )
                continue

            benchmark(
                'standard encode',
                size,
                lambda payload=payload: hashcodecs_base64.standard_b64encode(payload),
                (
                    ('stdlib', lambda payload=payload: stdlib_base64.b64encode(payload)),
                    ('pybase64', lambda payload=payload: pybase64.standard_b64encode(payload)),
                ),
            )
            benchmark(
                'standard decode',
                size,
                lambda standard=standard: hashcodecs_base64.b64decode(standard, validate=True),
                (
                    (
                        'stdlib',
                        lambda standard=standard: stdlib_base64.b64decode(standard, validate=True),
                    ),
                    (
                        'pybase64',
                        lambda standard=standard: pybase64.b64decode(standard, validate=True),
                    ),
                ),
            )
            benchmark(
                'URL-safe encode',
                size,
                lambda payload=payload: hashcodecs_base64.urlsafe_b64encode(payload),
                (
                    ('stdlib', lambda payload=payload: stdlib_base64.urlsafe_b64encode(payload)),
                    ('pybase64', lambda payload=payload: pybase64.urlsafe_b64encode(payload)),
                ),
            )
            benchmark(
                'URL-safe decode',
                size,
                lambda urlsafe=urlsafe: hashcodecs_base64.b64decode(urlsafe, b'-_', validate=True),
                (
                    (
                        'stdlib',
                        lambda urlsafe=urlsafe: stdlib_base64.b64decode(urlsafe, b'-_', validate=True),
                    ),
                    (
                        'pybase64',
                        lambda urlsafe=urlsafe: pybase64.b64decode(urlsafe, b'-_', validate=True),
                    ),
                ),
            )
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
