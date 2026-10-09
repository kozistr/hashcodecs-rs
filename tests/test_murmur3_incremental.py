from collections.abc import Callable
from typing import Any

import pytest

import hashcodecs
import hashcodecs.murmur3 as murmur3


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
def test_hasher_metadata_and_digest_formats(
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

    assert constructor().digest() == one_shot(b'')

    for length in (3, 4, 15, 16, 17):
        prefix = b'p' * length
        original = constructor(prefix, seed=42)
        snapshot = original.copy()
        assert original.digest() == one_shot(prefix, 42)
        assert original.update(b'') is None
        original.update(b'-original')
        snapshot.update(b'-snapshot')
        assert original.digest() == one_shot(prefix + b'-original', 42)
        assert snapshot.digest() == one_shot(prefix + b'-snapshot', 42)


def test_chunked_updates_match_one_shot_at_block_boundaries() -> None:
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
def test_hasher_rejects_invalid_inputs(constructor: Callable[..., Any]) -> None:
    with pytest.raises(TypeError):
        constructor([1, 2, 3])
    with pytest.raises(OverflowError):
        constructor(seed=-1)
    with pytest.raises(TypeError):
        constructor().update([1, 2, 3])
