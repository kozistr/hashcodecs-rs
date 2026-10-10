"""Run the published Python chart workloads, with progress and per-run logs."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from queue import Queue
from time import perf_counter

from _support import add_timing_arguments, configure_timing, nonnegative_int

JOBS = {
    'base64': [
        ('base64', 'python_base64.py', []),
        ('base64-into', 'python_base64.py', ['--into']),
        ('base64-mutable', 'python_base64.py', ['--bytearray-input']),
        ('base64-memoryview', 'python_base64.py', ['--memoryview-input']),
        ('base64-sliced-memoryview', 'python_base64.py', ['--sliced-memoryview-input']),
        ('base64-str', 'python_base64.py', ['--str-input']),
        ('base64-lenient', 'python_base64.py', ['--lenient']),
        ('base64-custom-lenient', 'python_base64.py', ['--custom-lenient']),
        ('base64-batch', 'python_base64_batch.py', []),
        ('base64-batch-memoryview', 'python_base64_batch.py', ['--memoryview-input', '--hashcodecs-only']),
        ('base64-batch-large', 'python_base64_batch.py', ['--large', '--hashcodecs-only']),
        (
            'base64-batch-memoryview-large',
            'python_base64_batch.py',
            ['--large', '--memoryview-input', '--hashcodecs-only'],
        ),
    ],
    'murmur3': [
        ('murmur3', 'python_murmur3.py', []),
        ('murmur3-incremental', 'python_murmur3.py', ['--incremental']),
        ('murmur3-mutable', 'python_murmur3.py', ['--bytearray-input', '--hashcodecs-only']),
        (
            'murmur3-mutable-incremental',
            'python_murmur3.py',
            ['--bytearray-input', '--incremental', '--hashcodecs-only'],
        ),
    ],
    'xxh3': [('xxhash', 'python_xxhash.py', [])],
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--groups', choices=JOBS, nargs='+', default=list(JOBS))
    parser.add_argument('--cpus', type=nonnegative_int, nargs='+', help='parallel exploration on these logical CPUs')
    parser.add_argument('--output', type=Path, help='log directory (default: target/python-benchmarks/<mode>)')
    parser.add_argument('--list', action='store_true', help='list workloads without running them')
    add_timing_arguments(parser)
    arguments = parser.parse_args()
    if arguments.cpus is not None and not arguments.quick:
        parser.error('--cpus requires --quick; publish measurements from serial runs')
    if arguments.cpus is not None and arguments.cpu is not None:
        parser.error('choose --cpu or --cpus')
    if arguments.cpus is not None and len(set(arguments.cpus)) != len(arguments.cpus):
        parser.error('--cpus must contain distinct CPUs')
    configure_timing(arguments)
    jobs = [job for group in dict.fromkeys(arguments.groups) for job in JOBS[group]]
    if arguments.list:
        for name, script, flags in jobs:
            print(f'{name}: {script} {" ".join(flags)}'.rstrip())
        return

    mode = 'quick' if arguments.quick else 'publication'
    output = arguments.output or Path('target/python-benchmarks') / mode
    output.mkdir(parents=True, exist_ok=True)
    cpus: Queue[int | None] = Queue()
    for cpu in arguments.cpus or [arguments.cpu]:
        cpus.put(cpu)
    workers = cpus.qsize()
    print(f'{len(jobs)} runs, {workers} worker(s), {mode} timing; logs: {output}', flush=True)

    def run(job: tuple[str, str, list[str]]) -> dict[str, object]:
        name, script, flags = job
        cpu = cpus.get()
        command = [
            sys.executable,
            '-u',
            str(Path(__file__).with_name(script)),
            *flags,
            '--samples',
            str(arguments.samples),
            '--minimum-sample-seconds',
            str(arguments.minimum_sample_seconds),
        ]
        if arguments.quick:
            command.append('--quick')
        if cpu is not None:
            command.extend(('--cpu', str(cpu)))
        started = perf_counter()
        try:
            with (output / f'{name}.log').open('w', encoding='utf-8') as log:
                result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False)
        finally:
            cpus.put(cpu)
        seconds = perf_counter() - started
        print(f'{name}: {seconds:.1f}s, status={result.returncode}', flush=True)
        return {'name': name, 'command': command, 'seconds': seconds, 'returncode': result.returncode}

    started = perf_counter()
    with ThreadPoolExecutor(max_workers=workers) as executor:
        results = list(executor.map(run, jobs))
    elapsed = perf_counter() - started
    (output / 'manifest.json').write_text(
        json.dumps({'mode': mode, 'workers': workers, 'seconds': elapsed, 'runs': results}, indent=2), encoding='utf-8'
    )
    print(f'Completed in {elapsed / 60:.1f} minutes.', flush=True)
    if any(result['returncode'] for result in results):
        raise SystemExit(f'benchmark failed; inspect logs in {output}')


if __name__ == '__main__':
    main()
