import inspect
from collections.abc import Callable

import pytest

import hashcodecs
import hashcodecs.xxhash as xxhash


def test_functions_keep_public_module_metadata() -> None:
    for name in xxhash.__all__:
        assert getattr(xxhash, name).__module__ == 'hashcodecs.xxhash'


def test_known_empty_digests_and_exports() -> None:
    assert hashcodecs.xxh3_64(b'') == 0x2D06800538D394C2
    assert hashcodecs.xxh3_64(bytearray()) == 0x2D06800538D394C2
    assert xxhash.xxh3_64(b'') == 0x2D06800538D394C2
    assert hashcodecs.xxh3_128(b'') == 0x99AA06D3014798D86001C324468D497F


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_one_shot_arguments_follow_public_signature(function: Callable[..., object]) -> None:
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


@pytest.mark.parametrize(
    ('bits', 'expected'),
    [(64, 0x241E5D5372565724), (128, 0xF7209D113313B8877E4D66691B80364D)],
)
def test_maximum_seed_remains_valid_across_apis(bits: int, expected: int) -> None:
    one_shot = getattr(hashcodecs, f'xxh3_{bits}')
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch_into = getattr(hashcodecs, f'xxh3_{bits}_batch_into')
    seed = (1 << 64) - 1
    output = bytearray(bits // 8)
    assert one_shot(b'hello', seed) == expected
    assert one_shot(s=b'hello', seed=seed) == expected
    assert batch([b'hello'], seed) == [expected]
    assert batch_into([b'hello'], output, seed) == len(output)
    assert int.from_bytes(output, 'little') == expected

    class IndexSeed:
        def __index__(self) -> int:
            return 42

    for function, args in (
        (one_shot, (b'hello',)),
        (batch, ([b'hello'],)),
        (batch_into, ([b'hello'], output)),
    ):
        for invalid in (-1, 1 << 64):
            with pytest.raises(OverflowError):
                function(*args, seed=invalid)
        for invalid in (None, 1.0, IndexSeed()):
            with pytest.raises(TypeError):
                function(*args, seed=invalid)

    assert int.from_bytes(output, 'little') == expected


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_one_shot_rejects_non_buffers(function: Callable[..., object]) -> None:
    with pytest.raises(TypeError):
        function([1, 2, 3])
