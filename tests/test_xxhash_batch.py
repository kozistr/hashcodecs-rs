import inspect
import subprocess
import sys
from array import array
from collections.abc import Callable
from pathlib import Path

import pytest

import hashcodecs
import hashcodecs.xxhash as xxhash

dynamic_xxh3_64_batch: Callable[..., list[int]] = hashcodecs.xxh3_64_batch

dynamic_xxh3_128_batch: Callable[..., list[int]] = hashcodecs.xxh3_128_batch


@pytest.mark.skipif(sys.version_info >= (3, 12), reason='requires synchronous allocation GC in CPython 3.10/3.11')
@pytest.mark.parametrize('bits', [64, 128])
@pytest.mark.parametrize('kind', ['bytes', 'bytearray', 'memoryview', 'mixed'])
def test_batch_survives_gc_finalizers(bits: int, kind: str) -> None:
    # A regression can read freed memory, so isolate it from the pytest process.
    result = subprocess.run(
        [sys.executable, str(Path(__file__).with_name('xxhash_gc.py')), str(bits), kind],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr


@pytest.mark.parametrize('item_count', range(2, 9))
def test_long_batch_remainders_match_one_shot(item_count: int) -> None:
    large = [bytes((index * 31 + item) & 0xFF for index in range(4097)) for item in range(item_count)]
    for one_shot, batch, batch_into, digest_size in (
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch, hashcodecs.xxh3_128_batch_into, 16),
    ):
        expected = [one_shot(value, 0x12345678) for value in large]
        assert batch(large, 0x12345678) == expected

        output = bytearray(digest_size * item_count)
        assert batch_into(large, output, 0x12345678) == len(output)
        assert output == b''.join(value.to_bytes(digest_size, 'little') for value in expected)


@pytest.mark.parametrize('bits', [64, 128])
@pytest.mark.parametrize(
    ('item_size', 'item_count', 'kind'),
    # Detachment starts at 1 MiB total input or 16K items. Counts around 64
    # also cross the stack limit for retained packed inputs.
    [(64, count, kind) for count in (16383, 16384, 16385) for kind in ('bytes', 'memoryview', 'bytearray')]
    + [(16385, count, kind) for count in (63, 64, 65, 127, 128, 129) for kind in ('bytes', 'memoryview')]
    + [(0, count, kind) for count in (16383, 16384, 16385) for kind in ('bytes', 'bytearray')],
)
def test_batch_results_at_detachment_boundaries(bits: int, kind: str, item_size: int, item_count: int) -> None:
    one_shot = getattr(hashcodecs, f'xxh3_{bits}')
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch_into = getattr(hashcodecs, f'xxh3_{bits}_batch_into')
    payloads = [bytes([index % 251]) * item_size for index in range(item_count)]
    items = (
        payloads
        if kind == 'bytes'
        else [memoryview(item) if kind == 'memoryview' else bytearray(item) for item in payloads]
    )
    expected = [one_shot(item, 42) for item in payloads]
    assert batch(items, 42) == expected
    packed = b''.join(value.to_bytes(bits // 8, 'little') for value in expected)
    output = bytearray(len(packed)) + b'untouched'
    assert batch_into(items, output, 42) == len(packed)
    assert output == packed + b'untouched'

    too_small = bytearray(b'?' * (len(packed) - 1))
    with pytest.raises(ValueError, match='destination has'):
        batch_into(items, too_small, 42)
    assert too_small == b'?' * (len(packed) - 1)


def test_batch_rejects_invalid_items_and_seeds() -> None:
    with pytest.raises(TypeError):
        dynamic_xxh3_64_batch((b'a', b'b'))
    with pytest.raises(TypeError, match='items element must be a bytes-like object'):
        dynamic_xxh3_128_batch([b'valid', object()])
    with pytest.raises(OverflowError):
        hashcodecs.xxh3_64(b'value', -1)
    with pytest.raises(OverflowError):
        hashcodecs.xxh3_128_batch([b'value'], 1 << 64)


@pytest.mark.parametrize(
    ('batch', 'batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64_batch, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128_batch, hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_batch_into_packs_little_endian_and_preserves_tail(
    batch: Callable[..., list[int]],
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    values = [b'', bytearray(b'hello'), memoryview(b'xxhash'), array('B', b'array')]
    one_shot = hashcodecs.xxh3_64 if digest_size == 8 else hashcodecs.xxh3_128
    expected = [one_shot(value, 42) for value in values]
    assert batch(values, 42) == expected
    tail = b'unchanged'
    output = bytearray(digest_size * len(values)) + bytearray(tail)

    assert str(inspect.signature(batch_into)) == '(items, output, seed=0)'
    assert batch_into(items=values, output=output, seed=42) == digest_size * len(values)
    assert output[: -len(tail)] == b''.join(value.to_bytes(digest_size, 'little') for value in expected)
    assert output[-len(tail) :] == tail


@pytest.mark.parametrize(
    ('batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_batch_into_preserves_output_on_failure(
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    too_small = bytearray(b'preserve')
    before = too_small[:]
    with pytest.raises(ValueError, match='destination has 8'):
        batch_into([b'a', b'b'], too_small)
    assert too_small == before

    output = bytearray(digest_size * 2)
    before = output[:]
    with pytest.raises(TypeError, match='items element must be a bytes-like object'):
        batch_into([b'valid', object()], output)
    assert output == before

    with pytest.raises(TypeError):
        batch_into([b'value'], bytes(digest_size))
    with pytest.raises(OverflowError):
        batch_into([b'value'], bytearray(digest_size), -1)


@pytest.mark.parametrize('bits', [64, 128])
def test_empty_batches_preserve_unused_output(bits: int) -> None:
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch_into = getattr(hashcodecs, f'xxh3_{bits}_batch_into')
    assert batch([]) == []
    assert batch_into([], bytearray()) == 0
    output = bytearray(b'untouched')
    assert batch_into([], output) == 0
    assert output == b'untouched'


def test_batch_into_exports() -> None:
    assert hashcodecs.xxh3_64_batch_into is xxhash.xxh3_64_batch_into
    assert hashcodecs.xxh3_128_batch_into is xxhash.xxh3_128_batch_into
