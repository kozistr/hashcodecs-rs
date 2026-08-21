import inspect
from collections.abc import Callable
from typing import Any

import hashcodecs
import hashcodecs.murmur3 as murmur3
import pytest


def test_murmur3_known_answers_and_buffer_inputs() -> None:
    assert hashcodecs.murmur3_32(b'hello') == 0x248BFA47
    assert murmur3.murmur3_32(b'hello') == 0x248BFA47
    assert hashcodecs.murmur3_32(bytearray(b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_32(memoryview(b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_x86_128_digest(bytes([1, 2, 3])) == bytes.fromhex('e16401f6334213b5334213b5334213b5')
    assert hashcodecs.murmur3_x64_128_digest(bytes([1, 2, 3])) == bytes.fromhex('a937130eef3e641a659a233c404a4e49')


@pytest.mark.parametrize(
    'function',
    [hashcodecs.murmur3_32, hashcodecs.murmur3_x86_128_digest, hashcodecs.murmur3_x64_128_digest],
)
def test_murmur3_one_shot_argument_compatibility(function: Callable[..., object]) -> None:
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
        function(b'hello', 1 << 32)


@pytest.mark.parametrize(
    ('constructor', 'one_shot', 'name', 'digest_size', 'block_size'),
    [
        (
            murmur3.murmur3_x86_32,
            lambda data, seed=0: murmur3.murmur3_32(data, seed).to_bytes(4, 'little'),
            'murmur3_x86_32',
            4,
            4,
        ),
        (murmur3.murmur3_x86_128, murmur3.murmur3_x86_128_digest, 'murmur3_x86_128', 16, 16),
        (murmur3.murmur3_x64_128, murmur3.murmur3_x64_128_digest, 'murmur3_x64_128', 16, 16),
    ],
)
def test_hashlib_style_murmur3_api(
    constructor: Callable[..., Any],
    one_shot: Callable[..., bytes],
    name: str,
    digest_size: int,
    block_size: int,
) -> None:
    hasher = constructor(memoryview(b'prefix'), 42)
    assert hasher.update(bytearray(b'-suffix')) is None
    expected = one_shot(b'prefix-suffix', 42)
    assert hasher.digest() == expected
    assert hasher.hexdigest() == expected.hex()
    assert hasher.digest() == expected
    assert hasher.name == name
    assert hasher.digest_size == digest_size
    assert hasher.block_size == block_size

    snapshot = constructor(b'prefix', seed=42).copy()
    snapshot.update(b'-suffix')
    assert snapshot.digest() == expected
    assert constructor().digest() == one_shot(b'')


def test_incremental_murmur3_matches_one_shot_at_every_boundary() -> None:
    constructors = (
        (
            hashcodecs.murmur3_x86_32,
            lambda data, seed: hashcodecs.murmur3_32(data, seed).to_bytes(4, 'little'),
        ),
        (hashcodecs.murmur3_x86_128, hashcodecs.murmur3_x86_128_digest),
        (hashcodecs.murmur3_x64_128, hashcodecs.murmur3_x64_128_digest),
    )
    for length in range(65):
        payload = bytes((index * 43 + 5) & 0xFF for index in range(length))
        for constructor, one_shot in constructors:
            actual = constructor(seed=0xFEEDBEEF)
            for offset in range(0, length, 3):
                actual.update(payload[offset : offset + 3])
            assert actual.digest() == one_shot(payload, 0xFEEDBEEF)


@pytest.mark.parametrize('constructor', [murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128])
def test_hashlib_style_murmur3_rejects_invalid_inputs(constructor: Callable[..., Any]) -> None:
    with pytest.raises(TypeError):
        constructor([1, 2, 3])
    with pytest.raises(OverflowError):
        constructor(seed=-1)
    with pytest.raises(TypeError):
        constructor().update([1, 2, 3])


def test_large_murmur3_calls_cross_the_gil_release_threshold() -> None:
    payload = bytes(range(256)) * 257
    assert hashcodecs.murmur3_32(payload, 42) == hashcodecs.murmur3_32(bytearray(payload), 42)
    assert hashcodecs.murmur3_x86_128_digest(payload, 42) == hashcodecs.murmur3_x86_128_digest(bytearray(payload), 42)
    assert hashcodecs.murmur3_x64_128_digest(payload, 42) == hashcodecs.murmur3_x64_128_digest(bytearray(payload), 42)
    for constructor in (murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128):
        hasher = constructor(seed=42)
        hasher.update(bytearray(payload))
        assert hasher.digest() == constructor(payload, 42).digest()
