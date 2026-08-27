"""Compare the Python Base64 API against CPython and pybase64."""

from __future__ import annotations

import argparse
import base64 as stdlib_base64
import gc
import sys
from collections.abc import Callable

import pybase64
from _support import SIZES, add_timing_arguments, configure_timing, data, pin_to_one_cpu, throughput

import hashcodecs.base64 as hashcodecs_base64


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
    mode.add_argument(
        '--bytearray-input',
        action='store_true',
        help='time hashcodecs with mutable bytearray inputs',
    )
    mode.add_argument(
        '--memoryview-input',
        action='store_true',
        help='time hashcodecs with full immutable memoryview inputs',
    )
    mode.add_argument(
        '--sliced-memoryview-input',
        action='store_true',
        help='time hashcodecs with nonzero-offset immutable memoryview inputs',
    )
    mode.add_argument(
        '--lenient',
        action='store_true',
        help='time MIME-wrapped and noisy lenient decoding',
    )
    mode.add_argument(
        '--advanced',
        action='store_true',
        help='time Python 3.15 ignorechars, canonical, and custom-alphabet decoding',
    )
    add_timing_arguments(parser)
    args = parser.parse_args()
    if args.advanced and sys.version_info < (3, 15):
        parser.error('--advanced requires Python 3.15 or newer')
    configure_timing(args.samples, args.minimum_sample_seconds)

    pin_to_one_cpu()
    gc.disable()
    try:
        for size in SIZES:
            payload = data(size)
            standard = stdlib_base64.b64encode(payload)
            urlsafe = stdlib_base64.urlsafe_b64encode(payload)

            if args.advanced:
                noisy = b'!'.join(standard[offset : offset + 76] for offset in range(0, len(standard), 76))
                unpadded = standard.rstrip(b'=')
                custom = noisy.translate(bytes.maketrans(b'+/', b'@#'))
                decoded_output = bytearray(size)
                benchmark(
                    'ignorechars decode',
                    size,
                    lambda noisy=noisy: hashcodecs_base64.b64decode(noisy, ignorechars=b'!'),
                    (('stdlib', lambda noisy=noisy: stdlib_base64.b64decode(noisy, ignorechars=b'!')),),
                )
                benchmark(
                    'canonical decode',
                    size,
                    lambda unpadded=unpadded: hashcodecs_base64.b64decode(unpadded, padded=False, canonical=True),
                    (
                        (
                            'stdlib',
                            lambda unpadded=unpadded: stdlib_base64.b64decode(unpadded, padded=False, canonical=True),
                        ),
                    ),
                )
                benchmark(
                    'custom decode',
                    size,
                    lambda custom=custom: hashcodecs_base64.b64decode(custom, b'@#', ignorechars=b'!'),
                    (
                        (
                            'stdlib',
                            lambda custom=custom: stdlib_base64.b64decode(custom, b'@#', ignorechars=b'!'),
                        ),
                    ),
                )
                benchmark_into(
                    'custom decode into',
                    size,
                    lambda custom=custom, output=decoded_output: hashcodecs_base64.b64decode_into(
                        custom, output, b'@#', ignorechars=b'!'
                    ),
                    decoded_output,
                    payload,
                )
                continue

            if args.lenient:
                mime = b'\r\n'.join(standard[offset : offset + 76] for offset in range(0, len(standard), 76))
                noisy = b'!'.join(standard[offset : offset + 76] for offset in range(0, len(standard), 76))
                for label, encoded in (('MIME', mime), ('noisy', noisy)):
                    decoded_output = bytearray(size)
                    benchmark(
                        f'{label} decode',
                        size,
                        lambda encoded=encoded: hashcodecs_base64.b64decode(encoded),
                        (
                            ('stdlib', lambda encoded=encoded: stdlib_base64.b64decode(encoded)),
                            ('pybase64', lambda encoded=encoded: pybase64.b64decode(encoded)),
                        ),
                    )
                    benchmark_into(
                        f'{label} decode into',
                        size,
                        lambda encoded=encoded, output=decoded_output: hashcodecs_base64.b64decode_into(
                            encoded, output
                        ),
                        decoded_output,
                        payload,
                    )
                continue

            if args.bytearray_input:
                mutable_payload = bytearray(payload)
                mutable_standard = bytearray(standard)
                encoded_output = bytearray(len(standard))
                decoded_output = bytearray(size)
                benchmark_ours(
                    'bytearray encode',
                    size,
                    lambda payload=mutable_payload: hashcodecs_base64.b64encode(payload),
                    standard,
                )
                benchmark_ours(
                    'bytearray decode',
                    size,
                    lambda standard=mutable_standard: hashcodecs_base64.b64decode(standard, validate=True),
                    payload,
                )
                benchmark_into(
                    'bytearray encode into',
                    size,
                    lambda payload=mutable_payload, output=encoded_output: hashcodecs_base64.b64encode_into(
                        payload, output
                    ),
                    encoded_output,
                    standard,
                )
                benchmark_into(
                    'bytearray decode into',
                    size,
                    lambda standard=mutable_standard, output=decoded_output: hashcodecs_base64.b64decode_into(
                        standard, output, validate=True
                    ),
                    decoded_output,
                    payload,
                )
                continue

            if args.memoryview_input or args.sliced_memoryview_input:
                if args.sliced_memoryview_input:
                    payload_owner = b'\xa5' + payload + b'\x5a'
                    standard_owner = b'!' + standard + b'!'
                    payload_view = memoryview(payload_owner)[1:-1]
                    standard_view = memoryview(standard_owner)[1:-1]
                    input_label = 'sliced memoryview'
                else:
                    payload_view = memoryview(payload)
                    standard_view = memoryview(standard)
                    input_label = 'memoryview'
                encoded_output = bytearray(len(standard))
                decoded_output = bytearray(size)
                benchmark_ours(
                    f'{input_label} encode',
                    size,
                    lambda payload=payload_view: hashcodecs_base64.b64encode(payload),
                    standard,
                )
                benchmark_ours(
                    f'{input_label} decode',
                    size,
                    lambda standard=standard_view: hashcodecs_base64.b64decode(standard, validate=True),
                    payload,
                )
                benchmark_into(
                    f'{input_label} encode into',
                    size,
                    lambda payload=payload_view, output=encoded_output: hashcodecs_base64.b64encode_into(
                        payload, output
                    ),
                    encoded_output,
                    standard,
                )
                benchmark_into(
                    f'{input_label} decode into',
                    size,
                    lambda standard=standard_view, output=decoded_output: hashcodecs_base64.b64decode_into(
                        standard, output, validate=True
                    ),
                    decoded_output,
                    payload,
                )
                continue

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
