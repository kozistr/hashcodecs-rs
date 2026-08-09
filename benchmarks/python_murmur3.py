"""Compare the Python MurmurHash3 API against mmh3."""

from __future__ import annotations

import argparse
import gc
from collections.abc import Callable

import hashcodecs.murmur3 as hashcodecs_murmur3
import mmh3
from _support import SIZES, data, pin_to_one_cpu, throughput


def benchmark(
    name: str,
    input_size: int,
    ours: Callable[[], object],
    reference: Callable[[], object],
) -> None:
    assert ours() == reference()
    ours_rate = throughput(ours, input_size)
    reference_rate = throughput(reference, input_size)
    print(
        f"{name:12} {input_size // 1024:>6} KiB  "
        f"hashcodecs={ours_rate / 1024**3:6.2f} GiB/s  "
        f"mmh3={reference_rate / 1024**3:6.2f} GiB/s ({ours_rate / reference_rate:4.2f}x)"
    )


def benchmark_ours(name: str, input_size: int, ours: Callable[[], object]) -> None:
    ours_rate = throughput(ours, input_size)
    print(f"{name:12} {input_size // 1024:>6} KiB  hashcodecs={ours_rate / 1024**3:6.2f} GiB/s")


def incremental(constructor: Callable[[], object], payload: bytes) -> bytes:
    hasher = constructor()
    hasher.update(payload)
    return hasher.digest()


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--hashcodecs-only",
        action="store_true",
        help="time hashcodecs without timing mmh3",
    )
    parser.add_argument(
        "--incremental",
        action="store_true",
        help="time constructor, update, and digest instead of the one-shot functions",
    )
    args = parser.parse_args()

    pin_to_one_cpu()
    gc.disable()
    try:
        for size in SIZES:
            payload = data(size)
            if args.incremental:
                cases = (
                    (
                        "x86 32-bit",
                        lambda payload=payload: incremental(hashcodecs_murmur3.murmur3_x86_32, payload),
                        lambda payload=payload: incremental(mmh3.mmh3_32, payload),
                    ),
                    (
                        "x86 128-bit",
                        lambda payload=payload: incremental(hashcodecs_murmur3.murmur3_x86_128, payload),
                        lambda payload=payload: incremental(mmh3.mmh3_x86_128, payload),
                    ),
                    (
                        "x64 128-bit",
                        lambda payload=payload: incremental(hashcodecs_murmur3.murmur3_x64_128, payload),
                        lambda payload=payload: incremental(mmh3.mmh3_x64_128, payload),
                    ),
                )
            else:
                cases = (
                    (
                        "x86 32-bit",
                        lambda payload=payload: hashcodecs_murmur3.murmur3_32(payload),
                        lambda payload=payload: mmh3.mmh3_32_uintdigest(payload),
                    ),
                    (
                        "x86 128-bit",
                        lambda payload=payload: hashcodecs_murmur3.murmur3_x86_128_digest(payload),
                        lambda payload=payload: mmh3.mmh3_x86_128_digest(payload),
                    ),
                    (
                        "x64 128-bit",
                        lambda payload=payload: hashcodecs_murmur3.murmur3_x64_128_digest(payload),
                        lambda payload=payload: mmh3.mmh3_x64_128_digest(payload),
                    ),
                )
            for name, ours, reference in cases:
                if args.hashcodecs_only:
                    benchmark_ours(name, size, ours)
                else:
                    benchmark(name, size, ours, reference)
    finally:
        gc.enable()


if __name__ == "__main__":
    main()
