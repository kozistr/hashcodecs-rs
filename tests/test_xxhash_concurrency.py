import sys
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

import pytest

import hashcodecs

FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()

XXH3_DETACH_THRESHOLD = 256 * 1024

GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_large_xxh3_calls_release_the_gil(
    function: Callable[..., object],
    assert_releases_gil: GILProgressAssertion,
) -> None:
    payload = bytes(range(256)) * (XXH3_DETACH_THRESHOLD // 256)
    expected = function(payload, 42)
    assert_releases_gil(lambda: function(payload, 42), expected, 128)


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
@pytest.mark.parametrize(
    ('one_shot', 'batch', 'batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch, hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_large_xxh3_batches_release_the_gil(
    one_shot: Callable[..., int],
    batch: Callable[..., list[int]],
    batch_into: Callable[..., int],
    digest_size: int,
    assert_releases_gil: GILProgressAssertion,
) -> None:
    payload = bytes(range(256)) * (XXH3_DETACH_THRESHOLD // 256)
    # Keep each detached call long enough for the waiting worker to be
    # scheduled. Repeating one short batch many times leaves only tiny GIL
    # release windows, which a busy CI host can miss entirely.
    inputs = [payload] * 1024
    expected_hash = one_shot(payload, 42)
    expected = [expected_hash] * len(inputs)
    output = bytearray(digest_size * len(inputs))

    assert_releases_gil(lambda: batch(inputs, 42), expected, 4)
    assert_releases_gil(lambda: batch_into(inputs, output, 42), len(output), 4)
    assert output == expected_hash.to_bytes(digest_size, 'little') * len(inputs)


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
@pytest.mark.parametrize('bits', [64, 128])
@pytest.mark.parametrize('item_size', [0, 1, 64])
def test_high_cardinality_xxh3_batches_release_the_gil(
    bits: int, item_size: int, assert_releases_gil: GILProgressAssertion
) -> None:
    payload = b'x' * item_size
    items = [payload] * 16384
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch_into = getattr(hashcodecs, f'xxh3_{bits}_batch_into')
    expected_hash = getattr(hashcodecs, f'xxh3_{bits}')(payload, 42)
    output = bytearray(bits // 8 * len(items))
    assert_releases_gil(lambda: batch(items, 42), [expected_hash] * len(items), 4)
    assert_releases_gil(lambda: batch_into(items, output, 42), len(output), 4)
    assert output == expected_hash.to_bytes(bits // 8, 'little') * len(items)


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('use_memoryview', [False, True], ids=['bytearray', 'memoryview'])
def test_xxh3_mutable_input_race_is_serialized(use_memoryview: bool) -> None:
    size = 1024 * 1024
    first = b'a' * size
    second = b'b' * size
    value = bytearray(first)
    input_value = memoryview(value) if use_memoryview else value
    expected = {hashcodecs.xxh3_64(first), hashcodecs.xxh3_64(second)}
    start = Barrier(2)

    def hash_value() -> list[int]:
        start.wait()
        return [hashcodecs.xxh3_64(input_value) for _ in range(64)]

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


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
def test_xxh3_batch_mutable_input_race_is_serialized() -> None:
    size = 1024 * 1024
    first = b'a' * size
    second = b'b' * size
    value = bytearray(first)
    expected = {hashcodecs.xxh3_64(first), hashcodecs.xxh3_64(second)}
    start = Barrier(2)

    def hash_value() -> list[int]:
        start.wait()
        return [hashcodecs.xxh3_64_batch([value])[0] for _ in range(32)]

    def mutate_value() -> None:
        start.wait()
        for index in range(32):
            value[:] = first if index % 2 else second

    with ThreadPoolExecutor(max_workers=2) as executor:
        hashes_future = executor.submit(hash_value)
        mutate_future = executor.submit(mutate_value)
        hashes = hashes_future.result()
        mutate_future.result()

    assert set(hashes) <= expected


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('bits', [64, 128])
@pytest.mark.parametrize('count', [8, 16384])
def test_xxh3_exact_bytes_batches_retain_inputs_during_list_mutation(bits: int, count: int) -> None:
    first = b'a' * 16
    second = b'b' * 16
    items = [bytes(bytearray(first))] * count
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch_into = getattr(hashcodecs, f'xxh3_{bits}_batch_into')
    one_shot = getattr(hashcodecs, f'xxh3_{bits}')
    expected = {one_shot(first, 42), one_shot(second, 42)}
    width = bits // 8
    output = bytearray(width * count)
    start = Barrier(2)

    def hash_values() -> None:
        start.wait()
        for _ in range(32):
            hashes = batch(items, 42)
            assert len(hashes) == count
            assert set(hashes) <= expected
            assert batch_into(items, output, 42) == len(output)
            assert {
                int.from_bytes(output[index : index + width], 'little') for index in range(0, len(output), width)
            } <= expected

    def mutate_items() -> None:
        start.wait()
        for index in range(128):
            value = bytes(bytearray(first if index % 2 else second))
            items[:] = [value] * count

    with ThreadPoolExecutor(max_workers=2) as executor:
        hashes_future = executor.submit(hash_values)
        mutation_future = executor.submit(mutate_items)
        hashes_future.result()
        mutation_future.result()
