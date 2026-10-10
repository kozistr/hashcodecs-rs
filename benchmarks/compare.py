"""Compare built Base64 or XXH3 extensions with alternating timing order."""

from __future__ import annotations

import argparse
import base64
import gc
import importlib.util
from collections.abc import Callable
from pathlib import Path
from statistics import median
from time import perf_counter
from types import ModuleType

from support import add_timing_arguments, calibrate, configure_timing, data, pin_to_one_cpu, positive_int


def load_extension(name: str, path: Path) -> ModuleType:
    spec = importlib.util.spec_from_file_location(f'{name}._hashcodecs', path.resolve(strict=True))
    if spec is None or spec.loader is None:
        raise ValueError(f'cannot load extension: {path}')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def elapsed(operation: Callable[[], object], iterations: int) -> float:
    start = perf_counter()
    for _ in range(iterations):
        operation()
    return perf_counter() - start


def compare(
    operations: tuple[Callable[[], object], Callable[[], object]],
    samples: int,
    minimum_seconds: float,
    *,
    rates: bool = False,
) -> tuple[float, float]:
    iterations = [calibrate(operation, minimum_seconds) for operation in operations]
    measurements: tuple[list[float], list[float]] = ([], [])
    for sample in range(samples):
        for index in (0, 1) if sample % 2 == 0 else (1, 0):
            duration = elapsed(operations[index], iterations[index]) / iterations[index]
            measurements[index].append(1 / duration if rates else duration)
    return median(measurements[0]), median(measurements[1])


def compare_base64(args: argparse.Namespace, modules: tuple[ModuleType, ModuleType]) -> None:
    print(
        'kind,alphabet,operation,output,bytes,baseline_ns,candidate_ns,baseline_gib_s,candidate_gib_s,change_percent',
        flush=True,
    )
    for kind in args.kinds:
        for alphabet in args.alphabets:
            altchars = {'standard': None, 'urlsafe': b'-_', 'custom': b'@#'}[alphabet]
            for size in args.sizes:
                payload = data(size)
                encoded = base64.b64encode(payload, altchars)
                for operation in args.operations:
                    source, expected = (payload, encoded) if operation == 'encode' else (encoded, payload)
                    if operation == 'mime':
                        source = b'\r\n'.join(source[offset : offset + 76] for offset in range(0, len(source), 76))
                    if kind == 'str':
                        source = source.decode('ascii')
                    else:
                        source = {'bytes': bytes, 'bytearray': bytearray, 'memoryview': memoryview}[kind](source)
                    kwargs = {} if altchars is None else {'altchars': altchars}
                    if operation == 'decode':
                        kwargs['validate'] = True
                    for output in args.outputs:
                        name = 'b64encode' if operation == 'encode' else 'b64decode'
                        # Use the same address for both builds so alignment
                        # and cache placement cannot favor either kernel.
                        buffer = bytearray(len(expected))
                        calls = []
                        for module in modules:
                            function = getattr(module, name + ('_into' if output == 'into' else ''))
                            arguments = (source, buffer) if output == 'into' else (source,)
                            result = function(*arguments, **kwargs)
                            if output == 'into':
                                assert result == len(expected)
                                assert buffer == expected
                            else:
                                assert result == expected
                            calls.append(lambda f=function, a=arguments, kw=kwargs: f(*a, **kw))
                        before, after = compare((calls[0], calls[1]), args.samples, args.minimum_sample_seconds)
                        scale = size / 1024**3
                        print(
                            f'{kind},{alphabet},{operation},{output},{size},{before * 1e9:.3f},{after * 1e9:.3f},'
                            f'{scale / before:.3f},{scale / after:.3f},{(before / after - 1) * 100:+.2f}',
                            flush=True,
                        )


def compare_xxh3(arguments: argparse.Namespace, modules: tuple[ModuleType, ModuleType]) -> None:
    baseline, candidate = modules
    print('kind,count,bytes,bits,baseline_gib_s,candidate_gib_s,change_percent', flush=True)
    for kind in arguments.kinds:
        convert = {
            'bytes': bytes,
            'bytearray': bytearray,
            'memoryview': lambda value: memoryview(bytearray(value)),
        }[kind]
        for count in arguments.batch_counts:
            for size in arguments.sizes:
                items = [convert(data(size)) for _ in range(count)]
                for bits in (64, 128):
                    before = getattr(baseline, f'xxh3_{bits}_batch')
                    after = getattr(candidate, f'xxh3_{bits}_batch')
                    assert before(items, 42) == after(items, 42)
                    baseline_rate, candidate_rate = compare(
                        (
                            lambda function=before, items=items: function(items, 42),
                            lambda function=after, items=items: function(items, 42),
                        ),
                        arguments.samples,
                        arguments.minimum_sample_seconds,
                        rates=True,
                    )
                    scale = count * size / 1024**3
                    print(
                        f'{kind},{count},{size},{bits},{baseline_rate * scale:.2f},'
                        f'{candidate_rate * scale:.2f},{(candidate_rate / baseline_rate - 1) * 100:+.2f}',
                        flush=True,
                    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    codecs = parser.add_subparsers(dest='codec', required=True)
    base = codecs.add_parser('base64', help='compare allocating and reusable Base64 calls')
    base.add_argument('--sizes', type=int, nargs='+', default=[0, 16, 64, 256, 1024, 4096, 65536, 1048576, 8388608])
    base.add_argument('--kinds', nargs='+', choices=['bytes', 'bytearray', 'memoryview', 'str'], default=['bytes'])
    base.add_argument(
        '--alphabets', nargs='+', choices=['standard', 'urlsafe', 'custom'], default=['standard', 'urlsafe']
    )
    base.add_argument(
        '--operations', nargs='+', choices=['encode', 'decode', 'lenient', 'mime'], default=['encode', 'decode']
    )
    base.add_argument('--outputs', nargs='+', choices=['returned', 'into'], default=['returned', 'into'])
    xxh3 = codecs.add_parser('xxh3', help='compare returned XXH3 batch calls')
    xxh3.add_argument('--batch-counts', type=positive_int, nargs='+', default=[32])
    xxh3.add_argument('--sizes', type=positive_int, nargs='+', default=[64, 1024, 4096, 8192])
    xxh3.add_argument('--kinds', choices=['bytes', 'bytearray', 'memoryview'], nargs='+', default=['bytes'])
    for codec in (base, xxh3):
        codec.add_argument('baseline', type=Path)
        codec.add_argument('candidate', type=Path)
        add_timing_arguments(codec)
    arguments = parser.parse_args()
    configure_timing(arguments)
    if arguments.codec == 'base64':
        if any(size < 0 for size in arguments.sizes):
            parser.error('sizes must be nonnegative')
        if 'str' in arguments.kinds and 'encode' in arguments.operations:
            parser.error('str inputs require decoding operations')
    modules = (load_extension('baseline', arguments.baseline), load_extension('candidate', arguments.candidate))
    pin_to_one_cpu(arguments.cpu)
    gc.disable()
    try:
        if arguments.codec == 'base64':
            compare_base64(arguments, modules)
        else:
            compare_xxh3(arguments, modules)
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
