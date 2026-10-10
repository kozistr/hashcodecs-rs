"""Compare Python binding latency using two extensions and alternating timing order."""

from __future__ import annotations

import argparse
import csv
import gc
import sys
from array import array
from pathlib import Path

from _support import add_timing_arguments, data, pin_to_one_cpu
from compare_base64 import compare, load_extension

HASH_FUNCTIONS = (
    'murmur3_32',
    'murmur3_x86_128_digest',
    'murmur3_x64_128_digest',
    'xxh3_64',
    'xxh3_128',
)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', type=Path)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('--functions', nargs='+', choices=HASH_FUNCTIONS, default=['murmur3_32', 'xxh3_64'])
    parser.add_argument('--sizes', type=int, nargs='+', default=[0, 16, 64, 4096])
    parser.add_argument(
        '--kinds',
        nargs='+',
        choices=['bytes', 'bytearray', 'memoryview', 'strided', 'array'],
        default=['bytes', 'bytearray', 'memoryview'],
    )
    parser.add_argument('--calls', nargs='+', choices=['positional', 'seeded', 'keyword'], default=['positional'])
    parser.add_argument('--seed', type=int, default=42)
    parser.add_argument('--output', type=Path)
    add_timing_arguments(parser)
    args = parser.parse_args()
    if any(size < 0 for size in args.sizes):
        parser.error('sizes must be nonnegative')
    modules = (load_extension('baseline', args.baseline), load_extension('candidate', args.candidate))
    pin_to_one_cpu()
    rows = []
    print(sys.version, file=sys.stderr)
    print('function,kind,call,bytes,baseline_ns,candidate_ns,speedup_percent', flush=True)
    gc.disable()
    try:
        for name in args.functions:
            for size in args.sizes:
                source = data(size)
                interleaved = bytearray(len(source) * 2)
                interleaved[::2] = source
                inputs = {
                    'bytes': source,
                    'bytearray': bytearray(source),
                    'memoryview': memoryview(source),
                    'strided': memoryview(interleaved)[::2],
                    'array': array('B', source),
                }
                for kind in args.kinds:
                    value = inputs[kind]
                    for call in args.calls:
                        operations = []
                        for module in modules:
                            function = getattr(module, name)
                            if call == 'keyword':
                                operations.append(lambda f=function, v=value, seed=args.seed: f(s=v, seed=seed))
                            elif call == 'seeded':
                                operations.append(lambda f=function, v=value, seed=args.seed: f(v, seed))
                            else:
                                operations.append(lambda f=function, v=value: f(v))
                        assert operations[0]() == operations[1]()
                        before, after = compare(
                            (operations[0], operations[1]), args.samples, args.minimum_sample_seconds
                        )
                        row = (
                            name,
                            kind,
                            call,
                            size,
                            f'{before * 1e9:.3f}',
                            f'{after * 1e9:.3f}',
                            f'{(before / after - 1) * 100:+.2f}',
                        )
                        rows.append(row)
                        print(','.join(map(str, row)), flush=True)
    finally:
        gc.enable()
    if args.output:
        with args.output.open('w', newline='') as destination:
            writer = csv.writer(destination)
            writer.writerow(('function', 'kind', 'call', 'bytes', 'baseline_ns', 'candidate_ns', 'speedup_percent'))
            writer.writerows(rows)


if __name__ == '__main__':
    main()
