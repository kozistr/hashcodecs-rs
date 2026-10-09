import base64 as stdlib_base64
import sys
import threading
from collections.abc import Callable, Sequence
from typing import cast

import pytest

import hashcodecs.base64 as base64

FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()

BASE64_DETACH_THRESHOLD = 256 * 1024

GILProgressAssertion = Callable[[Callable[[], object], object, int], None]

PYTHON_315 = sys.version_info >= (3, 15)

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
def test_large_base64_calls_release_the_gil(assert_releases_gil: GILProgressAssertion) -> None:
    payload = bytes(range(256)) * (BASE64_DETACH_THRESHOLD // 256)
    encoded = stdlib_base64.b64encode(payload)

    assert_releases_gil(lambda: base64.b64encode(payload), encoded, 128)
    assert_releases_gil(lambda: base64.b64decode(encoded, validate=True), payload, 128)
    custom = encoded.translate(bytes.maketrans(b'+/', b'@#'))
    assert_releases_gil(lambda: base64.b64decode(custom, b'@#', validate=True), payload, 128)


def _assert_mutable_input_race_is_serialized(
    operation: Callable[[], bytes],
    value: bytearray,
    states: Sequence[bytes],
    expected: set[bytes],
) -> None:
    start = threading.Barrier(2)
    failures: list[BaseException | bytes] = []

    def run_operation() -> None:
        try:
            start.wait()
            for _ in range(32):
                result = operation()
                if result not in expected:
                    failures.append(result[:64])
                    return
        except BaseException as error:
            failures.append(error)

    def resize_input() -> None:
        start.wait()
        for index in range(32):
            value[:] = states[index % 2]

    worker = threading.Thread(target=run_operation)
    mutator = threading.Thread(target=resize_input)
    worker.start()
    mutator.start()
    worker.join(timeout=30)
    mutator.join(timeout=30)

    assert not worker.is_alive()
    assert not mutator.is_alive()
    assert not failures


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
def test_base64_bytearray_resize_races_are_serialized() -> None:
    raw_states = (b'a' * (1024 * 1024), b'b' * (1024 * 1024 + 3))
    raw = bytearray(raw_states[0])
    encoded_states = tuple(stdlib_base64.b64encode(state) for state in raw_states)
    _assert_mutable_input_race_is_serialized(
        lambda: base64.b64encode(raw),
        raw,
        raw_states,
        set(encoded_states),
    )

    encoded = bytearray(encoded_states[0])
    _assert_mutable_input_race_is_serialized(
        lambda: base64.b64decode(encoded, validate=True),
        encoded,
        encoded_states,
        set(raw_states),
    )

    raw = bytearray(raw_states[0])
    encode_output = bytearray(len(encoded_states[1]))
    _assert_mutable_input_race_is_serialized(
        lambda: bytes(encode_output[: base64.b64encode_into(raw, encode_output)]),
        raw,
        raw_states,
        set(encoded_states),
    )

    encoded = bytearray(encoded_states[0])
    decode_output = bytearray(len(raw_states[1]))
    _assert_mutable_input_race_is_serialized(
        lambda: bytes(decode_output[: base64.b64decode_into(encoded, decode_output, validate=True)]),
        encoded,
        encoded_states,
        set(raw_states),
    )


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('urlsafe', [False, True])
def test_free_threaded_encode_snapshot_respects_legacy_callback_order(urlsafe: bool) -> None:
    source = bytearray(b'abc')

    class Padded:
        def __bool__(self) -> bool:
            source[:] = b'def'
            return True

    function = cast(Callable[..., bytes], base64.urlsafe_b64encode if urlsafe else base64.b64encode)
    expected = b'ZGVm' if PYTHON_315 or urlsafe else b'YWJj'
    assert function(source, padded=Padded()) == expected


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('reusable', [False, True])
def test_free_threaded_decode_snapshots_ignorechars_after_canonical_callback(reusable: bool) -> None:
    ignorechars = bytearray(b'!')

    class Canonical:
        def __bool__(self) -> bool:
            ignorechars[:] = b'?'
            return False

    output = bytearray(3)
    if reusable:
        assert dynamic_b64decode_into(b'Y?WJj', output, ignorechars=ignorechars, canonical=Canonical()) == 3
        assert output == b'abc'
    else:
        assert dynamic_b64decode(b'Y?WJj', ignorechars=ignorechars, canonical=Canonical()) == b'abc'


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
def test_free_threaded_standard_into_snapshot() -> None:
    source = bytearray(b'YWJj')
    output = bytearray(b'.....')

    assert base64.standard_b64decode_into(source, output) == 3
    assert source == b'YWJj'
    assert output == b'abc..'


@pytest.mark.parametrize('length', [256 * 1024 - 1, 256 * 1024, 256 * 1024 + 1])
def test_gil_boundary(length: int) -> None:
    payload = bytes((index * 17 + 3) & 0xFF for index in range(length))
    expected = stdlib_base64.b64encode(payload)
    assert base64.b64encode(payload) == expected
    assert base64.b64decode(expected, validate=True) == payload


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
def test_large_base64_batch_releases_the_gil(assert_releases_gil: GILProgressAssertion) -> None:
    payload = bytes(range(256)) * (BASE64_DETACH_THRESHOLD // 256)
    encoded_item = stdlib_base64.b64encode(payload)
    # Keep both operations long enough for the awakened worker to be scheduled
    # even on fast SIMD hosts while bounding the total test allocation.
    payloads = [payload] * 512
    encoded = [encoded_item] * 512

    assert_releases_gil(lambda: base64.b64encode_batch(payloads), encoded, 1)
    assert_releases_gil(lambda: base64.b64decode_batch(encoded, validate=True), payloads, 1)
