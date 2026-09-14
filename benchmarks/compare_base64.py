"""Compare two Base64 extensions on one CPU with alternating timing order."""

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

from _support import add_timing_arguments, data, pin_to_one_cpu


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
    operations: tuple[Callable[[], object], Callable[[], object]], samples: int, minimum_seconds: float
) -> tuple[float, float]:
    iterations = []
    for operation in operations:
        count = 1
        while elapsed(operation, count) < minimum_seconds:
            count *= 2
        iterations.append(count)
    times: tuple[list[float], list[float]] = ([], [])
    for sample in range(samples):
        for index in (0, 1) if sample % 2 == 0 else (1, 0):
            times[index].append(elapsed(operations[index], iterations[index]) / iterations[index])
    return median(times[0]), median(times[1])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--sizes', type=int, nargs='+', default=[0, 16, 64, 256, 1024, 4096, 65536, 1048576, 8388608])
    parser.add_argument('--kinds', nargs='+', choices=['bytes', 'bytearray', 'memoryview'], default=['bytes'])
    parser.add_argument(
        '--alphabets', nargs='+', choices=['standard', 'urlsafe', 'custom'], default=['standard', 'urlsafe']
    )
    parser.add_argument(
        '--operations', nargs='+', choices=['encode', 'decode', 'lenient', 'mime'], default=['encode', 'decode']
    )
    parser.add_argument('--outputs', nargs='+', choices=['returned', 'into'], default=['returned', 'into'])
    add_timing_arguments(parser)
    args = parser.parse_args()
    if any(size < 0 for size in args.sizes):
        parser.error('sizes must be nonnegative')
    modules = (load_extension('baseline', args.baseline), load_extension('candidate', args.candidate))
    pin_to_one_cpu()
    gc.disable()
    try:
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
                        source = {'bytes': bytes, 'bytearray': bytearray, 'memoryview': memoryview}[kind](source)
                        kwargs = {} if altchars is None else {'altchars': altchars}
                        if operation == 'decode':
                            kwargs['validate'] = True
                        for output in args.outputs:
                            name = 'b64encode' if operation == 'encode' else 'b64decode'
                            buffers = (bytearray(len(expected)), bytearray(len(expected)))
                            calls = []
                            for index, module in enumerate(modules):
                                function = getattr(module, name + ('_into' if output == 'into' else ''))
                                arguments = (source, buffers[index]) if output == 'into' else (source,)
                                result = function(*arguments, **kwargs)
                                if output == 'into':
                                    assert result == len(expected)
                                    assert buffers[index] == expected
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
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
