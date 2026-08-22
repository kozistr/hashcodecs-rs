import inspect
from collections.abc import Callable

import pytest

import hashcodecs
import hashcodecs.xxhash as xxhash


def test_xxh3_known_empty_digests_and_exports() -> None:
    assert hashcodecs.xxh3_64(b'') == 0x2D06800538D394C2
    assert xxhash.xxh3_64(b'') == 0x2D06800538D394C2
    assert hashcodecs.xxh3_128(b'') == 0x99AA06D3014798D86001C324468D497F


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_xxh3_one_shot_argument_compatibility(function: Callable[..., object]) -> None:
    assert str(inspect.signature(function)) == '(s, seed=0)'
    expected = function(b'hello', 42)
    assert function(s=b'hello', seed=42) == expected
    assert function(bytearray(b'hello'), seed=42) == expected
    assert function(memoryview(b'hello'), 42) == expected

    with pytest.raises(TypeError):
        function()
    with pytest.raises(TypeError):
        function(b'hello', s=b'hello')
    with pytest.raises(TypeError):
        function(b'hello', 42, seed=42)
    with pytest.raises(TypeError):
        function(b'hello', unknown=42)
    with pytest.raises(OverflowError):
        function(b'hello', -1)
    with pytest.raises(OverflowError):
        function(b'hello', 1 << 64)


def test_xxh3_batch_matches_one_shot_and_accepts_buffer_inputs() -> None:
    values = [b'', bytearray(b'hello'), memoryview(b'xxhash')]
    assert hashcodecs.xxh3_64_batch(values, 42) == [hashcodecs.xxh3_64(value, 42) for value in values]
    assert hashcodecs.xxh3_128_batch(values, 42) == [hashcodecs.xxh3_128(value, 42) for value in values]

    large = [bytes((index * 31 + item) & 0xFF for index in range(4097)) for item in range(8)]
    assert hashcodecs.xxh3_64_batch(large, 0x12345678) == [hashcodecs.xxh3_64(value, 0x12345678) for value in large]
    assert hashcodecs.xxh3_128_batch(large, 0x12345678) == [hashcodecs.xxh3_128(value, 0x12345678) for value in large]


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_xxh3_rejects_non_buffers(function: object) -> None:
    with pytest.raises(TypeError):
        function([1, 2, 3])  # type: ignore[operator]


def test_xxh3_rejects_invalid_batch_and_seed_inputs() -> None:
    with pytest.raises(TypeError):
        hashcodecs.xxh3_64_batch((b'a', b'b'))  # type: ignore[arg-type]
    with pytest.raises(TypeError, match='items element must be a bytes-like object'):
        hashcodecs.xxh3_128_batch([b'valid', object()])
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
def test_xxh3_batch_into_packs_little_endian_and_preserves_tail(
    batch: Callable[..., list[int]],
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    values = [b'', bytearray(b'hello'), memoryview(b'xxhash')]
    expected = batch(values, 42)
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
def test_xxh3_batch_into_empty_and_failure_atomicity(
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    assert batch_into([], bytearray()) == 0

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


@pytest.mark.parametrize(
    ('one_shot', 'batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_xxh3_batch_into_allows_output_to_alias_an_input(
    one_shot: Callable[..., int],
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    output = bytearray(b'input also serves as the reusable output')
    original = bytes(output)
    expected = one_shot(original, 42).to_bytes(digest_size, 'little')

    assert batch_into([output], output, 42) == digest_size
    assert output[:digest_size] == expected
    assert output[digest_size:] == original[digest_size:]


def test_xxh3_batch_into_exports() -> None:
    assert hashcodecs.xxh3_64_batch_into is xxhash.xxh3_64_batch_into
    assert hashcodecs.xxh3_128_batch_into is xxhash.xxh3_128_batch_into
