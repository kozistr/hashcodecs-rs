import sys
from collections.abc import Callable

import pytest

import hashcodecs


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_xxh3_exact_memoryviews_preserve_layout_and_owner_semantics(function: Callable[..., object]) -> None:
    for payload in (b'', b'hello', bytes(range(256)) * 16):
        padded = b'\xa5' + payload + b'\x5a'
        writable = bytearray(payload)
        interleaved = bytearray(len(payload) * 2)
        interleaved[::2] = payload
        views = (
            memoryview(payload),
            memoryview(padded)[1:-1],
            memoryview(writable),
            memoryview(writable).toreadonly(),
            memoryview(interleaved)[::2],
        )
        expected = function(payload)
        assert all(function(view) == expected for view in views)


@pytest.mark.parametrize('function', [hashcodecs.xxh3_64, hashcodecs.xxh3_128])
def test_xxh3_rejects_released_memoryviews(function: Callable[..., object]) -> None:
    released = memoryview(b'hello')
    released.release()
    with pytest.raises(ValueError, match='operation forbidden on released memoryview object'):
        function(released)


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires the Python-level buffer protocol')
def test_xxh3_acquires_generic_exporters_directly() -> None:
    class BufferHook:
        def __init__(self) -> None:
            self.calls = 0

        def __buffer__(self, flags: int) -> memoryview:
            self.calls += 1
            return memoryview(b'h.e.l.l.o.')[::2]

    value = BufferHook()
    assert hashcodecs.xxh3_64(value) == hashcodecs.xxh3_64(b'hello')
    assert value.calls == 1


@pytest.mark.parametrize(
    ('one_shot', 'batch'),
    [
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch),
    ],
)
def test_xxh3_batch_retains_large_exact_memoryview_owner(
    one_shot: Callable[..., int],
    batch: Callable[..., list[int]],
) -> None:
    owner = bytearray(bytes(range(256)) * 256)
    view = memoryview(owner)
    assert batch([view], 42) == [one_shot(bytes(owner), 42)]


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


@pytest.mark.parametrize(
    ('one_shot', 'batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_xxh3_batch_into_snapshots_overlapping_memoryviews(
    one_shot: Callable[..., int],
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    output = bytearray(range(64))
    original = bytes(output)
    inputs = [memoryview(output)[16:32], memoryview(output)[:16]]
    expected = b''.join(
        one_shot(value, 42).to_bytes(digest_size, 'little') for value in (original[16:32], original[:16])
    )

    assert batch_into(inputs, output, 42) == len(expected)
    assert output[: len(expected)] == expected
    assert output[len(expected) :] == original[len(expected) :]


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python-level buffer protocol support')
@pytest.mark.parametrize(
    ('one_shot', 'batch_into', 'digest_size'),
    [
        (hashcodecs.xxh3_64, hashcodecs.xxh3_64_batch_into, 8),
        (hashcodecs.xxh3_128, hashcodecs.xxh3_128_batch_into, 16),
    ],
)
def test_xxh3_batch_into_releases_custom_buffers_before_writing(
    one_shot: Callable[..., int],
    batch_into: Callable[..., int],
    digest_size: int,
) -> None:
    output = bytearray(b'.' * digest_size)
    events: list[str] = []

    class ReleaseBuffer:
        def __buffer__(self, flags: int) -> memoryview:
            events.append('acquire')
            return memoryview(b'payload')

        def __release_buffer__(self, view: memoryview) -> None:
            events.append('release')
            output[:] = b'X' * digest_size

    expected = one_shot(b'payload').to_bytes(digest_size, 'little')
    assert batch_into([ReleaseBuffer()], output) == digest_size
    assert events == ['acquire', 'release']
    assert output == expected
