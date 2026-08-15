import hashcodecs
import hashcodecs.xxhash as xxhash
import pytest


def test_xxh3_known_empty_digests_and_exports() -> None:
    assert hashcodecs.xxh3_64(b'') == 0x2D06800538D394C2
    assert xxhash.xxh3_64(b'') == 0x2D06800538D394C2
    assert hashcodecs.xxh3_128(b'') == 0x99AA06D3014798D86001C324468D497F


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
