"""Compare Base64 decoder builds on one CPU with alternating timing order."""

from __future__ import annotations

import argparse
import base64
import gc
from collections.abc import Callable, Iterator
from pathlib import Path
from types import ModuleType

from _support import add_timing_arguments, data, pin_to_one_cpu, positive_int
from compare_xxh3_batches import compare, load_extension


def cases(size: int) -> Iterator[tuple[str, bytes, dict[str, object]]]:
    encoded = base64.b64encode(data(size))

    yield 'standard', encoded, {}
    yield 'strict', encoded, {'validate': True}
    yield 'urlsafe', base64.b64encode(data(size), b'-_'), {'altchars': b'-_'}
    yield 'custom', base64.b64encode(data(size), b'@#'), {'altchars': b'@#'}
    yield 'custom-strict', base64.b64encode(data(size), b'@#'), {'altchars': b'@#', 'validate': True}
    yield 'configured', encoded, {'ignorechars': b'\r\n'}
    yield 'configured-custom', base64.b64encode(data(size), b'@#'), {'altchars': b'@#', 'ignorechars': b'\r\n'}
    yield 'mime', b'\r\n'.join(encoded[i : i + 76] for i in range(0, len(encoded), 76)), {}
    yield 'late-noise', encoded[:-4] + b'!!!!' + encoded[-4:], {}
    yield 'mostly-noise', b'!' * max(size, 4) + b'YWJj', {}
    yield 'configured-noise', b'!' * max(size, 4) + b'YWJj', {'ignorechars': b'!'}


def operation(
    module: ModuleType, encoded: bytes, options: dict[str, object], mode: str, kind: str, expected: bytes
) -> Callable[[], object]:
    value = {'bytes': bytes, 'bytearray': bytearray, 'memoryview': memoryview}[kind](encoded)
    assert module.b64decode(value, **options) == expected

    if mode == 'returned':
        return lambda: module.b64decode(value, **options)

    if mode == 'batch':
        items = [value] * 32
        return lambda: module.b64decode_batch(items, **options)

    output = bytearray(len(expected) + 8)

    assert module.b64decode_into(value, output, **options) == len(expected)
    assert output == expected + b'\0' * 8
    return lambda: module.b64decode_into(value, output, **options)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--sizes', type=positive_int, nargs='+', default=[64, 4096, 1048576])
    parser.add_argument('--cases', nargs='+')
    parser.add_argument('--modes', choices=['returned', 'into', 'batch'], nargs='+', default=['returned', 'into'])
    parser.add_argument('--kinds', choices=['bytes', 'bytearray', 'memoryview'], nargs='+', default=['bytes'])
    add_timing_arguments(parser)
    arguments = parser.parse_args()

    modules = (load_extension('baseline', arguments.baseline), load_extension('candidate', arguments.candidate))

    pin_to_one_cpu()
    gc.disable()

    try:
        print('case,kind,mode,bytes,baseline_ns,candidate_ns,change_percent', flush=True)
        for size in arguments.sizes:
            for name, encoded, options in cases(size):
                if arguments.cases and name not in arguments.cases:
                    continue

                expected = modules[0].b64decode(encoded, **options)
                for kind in arguments.kinds:
                    for mode in arguments.modes:
                        if mode == 'batch' and 'ignorechars' in options:
                            continue

                        rates = compare(
                            tuple(operation(module, encoded, options, mode, kind, expected) for module in modules),
                            arguments.samples,
                            arguments.minimum_sample_seconds,
                        )

                        print(
                            f'{name},{kind},{mode},{size},{1e9 / rates[0]:.2f},{1e9 / rates[1]:.2f},'
                            f'{(rates[1] / rates[0] - 1) * 100:+.2f}',
                            flush=True,
                        )
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
