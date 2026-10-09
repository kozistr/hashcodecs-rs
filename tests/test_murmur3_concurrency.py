import sys
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier
from typing import Any

import pytest

import hashcodecs
import hashcodecs.murmur3 as murmur3

FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()

GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
def test_large_murmur3_calls_release_the_gil(assert_releases_gil: GILProgressAssertion) -> None:
    payload = bytes(range(256)) * 257
    for function in (
        hashcodecs.murmur3_32,
        hashcodecs.murmur3_x86_128_digest,
        hashcodecs.murmur3_x64_128_digest,
    ):
        expected = function(payload, 42)
        assert_releases_gil(lambda function=function: function(payload, 42), expected, 256)

    for constructor in (murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128):
        expected = constructor(payload, 42).digest()

        def incremental_digest(constructor: Callable[..., Any] = constructor) -> bytes:
            hasher = constructor(seed=42)
            hasher.update(payload)
            return hasher.digest()

        assert_releases_gil(incremental_digest, expected, 256)


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('use_memoryview', [False, True], ids=['bytearray', 'memoryview'])
@pytest.mark.parametrize(
    ('one_shot', 'constructor'),
    [
        (hashcodecs.murmur3_32, murmur3.murmur3_x86_32),
        (hashcodecs.murmur3_x86_128_digest, murmur3.murmur3_x86_128),
        (hashcodecs.murmur3_x64_128_digest, murmur3.murmur3_x64_128),
    ],
)
def test_murmur3_mutable_input_races_are_serialized(
    one_shot: Callable[[object], object],
    constructor: Callable[..., Any],
    use_memoryview: bool,
) -> None:
    size = 1024 * 1024
    first = b'a' * size
    second = b'b' * size
    value = bytearray(first)
    input_value = memoryview(value) if use_memoryview else value
    expected = {one_shot(first), one_shot(second)}
    start = Barrier(2)

    def hash_value() -> list[object]:
        start.wait()
        results = []
        for _ in range(32):
            results.append(one_shot(input_value))
            hasher = constructor()
            hasher.update(input_value)
            results.append(
                hasher.digest() if not isinstance(results[-1], int) else int.from_bytes(hasher.digest(), 'little')
            )
        return results

    def mutate_value() -> None:
        start.wait()
        for index in range(64):
            value[:] = first if index % 2 else second

    with ThreadPoolExecutor(max_workers=2) as executor:
        hashes_future = executor.submit(hash_value)
        mutate_future = executor.submit(mutate_value)
        hashes = hashes_future.result()
        mutate_future.result()

    assert set(hashes) <= expected
