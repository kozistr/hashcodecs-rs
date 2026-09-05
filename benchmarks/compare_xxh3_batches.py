"""Compare two built XXH3 extensions on one CPU with alternating timing order."""

from __future__ import annotations

import argparse
import gc
import importlib.util
from collections.abc import Callable
from pathlib import Path
from statistics import median
from time import perf_counter
from types import ModuleType

from _support import add_timing_arguments, data, pin_to_one_cpu, positive_int


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
) -> tuple[float, float]:
    assert operations[0]() == operations[1]()
    iterations = []
    for operation in operations:
        count = 1
        while elapsed(operation, count) < minimum_seconds:
            count *= 2
        iterations.append(count)
    rates: tuple[list[float], list[float]] = ([], [])
    for sample in range(samples):
        for index in (0, 1) if sample % 2 == 0 else (1, 0):
            rates[index].append(iterations[index] / elapsed(operations[index], iterations[index]))
    return median(rates[0]), median(rates[1])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path, help='baseline _hashcodecs.pyd or .so')
    parser.add_argument('candidate', type=Path, help='candidate _hashcodecs.pyd or .so')
    parser.add_argument('--batch-counts', type=positive_int, nargs='+', default=[32])
    parser.add_argument('--sizes', type=positive_int, nargs='+', default=[64, 1024, 4096, 8192])
    parser.add_argument(
        '--kinds',
        choices=['bytes', 'bytearray', 'memoryview'],
        nargs='+',
        default=['bytes', 'bytearray', 'memoryview'],
    )
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    baseline = load_extension('baseline', arguments.baseline)
    candidate = load_extension('candidate', arguments.candidate)
    pin_to_one_cpu()
    gc.disable()
    try:
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
                        baseline_rate, candidate_rate = compare(
                            (
                                lambda function=before, items=items: function(items, 42),
                                lambda function=after, items=items: function(items, 42),
                            ),
                            arguments.samples,
                            arguments.minimum_sample_seconds,
                        )
                        scale = count * size / 1024**3
                        print(
                            f'{kind},{count},{size},{bits},{baseline_rate * scale:.2f},'
                            f'{candidate_rate * scale:.2f},{(candidate_rate / baseline_rate - 1) * 100:+.2f}',
                            flush=True,
                        )
    finally:
        gc.enable()


if __name__ == '__main__':
    main()
