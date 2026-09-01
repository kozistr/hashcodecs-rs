import base64 as stdlib_base64
import binascii
import inspect
import sys
from collections.abc import Callable

import pytest

import hashcodecs
import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)
FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()
ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError
BASE64_DETACH_THRESHOLD = 256 * 1024
GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


class _BatchList(list[object]):
    pass


class _ChangingBuffer:
    def __init__(self) -> None:
        self.calls = 0

    def __buffer__(self, flags: int) -> memoryview:
        self.calls += 1
        return memoryview(b'-_' if self.calls == 1 else b'@#')


def test_base64_batch_empty_single_heterogeneous_and_ordered() -> None:
    payloads = _BatchList([b'', bytearray(b'a'), memoryview(b'ab'), b'abc', bytes(range(256))])
    expected = [stdlib_base64.b64encode(payload) for payload in payloads]

    assert base64.b64encode_batch([]) == []
    assert base64.b64encode_batch([b'a']) == [b'YQ==']
    assert base64.b64encode_batch(payloads) == expected
    assert hashcodecs.b64encode_batch(payloads) == expected

    encoded = _BatchList([expected[0], bytearray(expected[1]), memoryview(expected[2]), expected[3].decode()])
    decoded = [b'', b'a', b'ab', b'abc']
    assert base64.b64decode_batch([]) == []
    assert base64.b64decode_batch(encoded, validate=True) == decoded
    assert hashcodecs.b64decode_batch(encoded, validate=True) == decoded


@pytest.mark.parametrize('batch_size', [8, 64, 1024])
def test_base64_batch_cardinalities_and_boundaries(batch_size: int) -> None:
    lengths = [0, 1, 2, 3, 15, 16, 31, 32, 47, 48, 63, 64, 65]
    payloads = [
        bytes((index * 37 + offset) & 0xFF for index in range(lengths[offset % len(lengths)]))
        for offset in range(batch_size)
    ]
    expected = [stdlib_base64.b64encode(payload) for payload in payloads]

    assert base64.b64encode_batch(payloads) == expected
    assert base64.b64decode_batch(expected, validate=True) == payloads

    encoded_outputs = [bytearray([0xA5] * (len(value) + 1)) for value in expected]
    assert base64.b64encode_batch_into(payloads, encoded_outputs) == [len(value) for value in expected]
    assert [bytes(output[:-1]) for output in encoded_outputs] == expected
    assert all(output[-1] == 0xA5 for output in encoded_outputs)

    decoded_outputs = [bytearray([0xA5] * (len(value) + 1)) for value in payloads]
    assert base64.b64decode_batch_into(expected, decoded_outputs, validate=True) == [len(value) for value in payloads]
    assert [bytes(output[:-1]) for output in decoded_outputs] == payloads
    assert all(output[-1] == 0xA5 for output in decoded_outputs)


def test_base64_batch_alphabets_and_wrappers() -> None:
    payloads = [b'', b'abc', b'\xfb\xff', bytes(range(64))]
    standard = [stdlib_base64.b64encode(payload) for payload in payloads]
    urlsafe = [stdlib_base64.urlsafe_b64encode(payload) for payload in payloads]
    custom = [stdlib_base64.b64encode(payload, b'@#') for payload in payloads]
    duplicate = [stdlib_base64.b64encode(payload, b'++') for payload in payloads]

    assert base64.standard_b64encode_batch(payloads) == standard
    assert base64.standard_b64decode_batch(standard) == payloads
    assert base64.urlsafe_b64encode_batch(payloads) == urlsafe
    assert base64.urlsafe_b64decode_batch(urlsafe) == payloads
    assert base64.b64encode_batch(payloads, b'@#') == custom
    assert base64.b64decode_batch(custom, '@#', validate=True) == payloads
    assert base64.b64encode_batch(payloads, b'++') == duplicate
    assert base64.b64decode_batch(duplicate, b'++') == [stdlib_base64.b64decode(value, b'++') for value in duplicate]
    assert base64.b64encode_batch(payloads, b'+/') == standard
    assert base64.b64decode_batch(standard, b'+/', validate=True) == payloads


def test_base64_batch_into_alphabets_wrappers_and_alias() -> None:
    payloads = _BatchList([b'', bytearray(b'abc'), memoryview(b'\xfb\xff'), bytes(range(64))])
    standard = [stdlib_base64.b64encode(payload) for payload in payloads]
    urlsafe = [stdlib_base64.urlsafe_b64encode(payload) for payload in payloads]
    custom = [stdlib_base64.b64encode(payload, b'@#') for payload in payloads]

    encoded_outputs = _BatchList([bytearray(len(value) + 2) for value in standard])
    assert base64.standard_b64encode_batch_into(payloads, encoded_outputs) == [len(value) for value in standard]
    assert [
        bytes(output[:length]) for output, length in zip(encoded_outputs, map(len, standard), strict=True)
    ] == standard

    assert base64.urlsafe_b64encode_batch_into(payloads, encoded_outputs) == [len(value) for value in urlsafe]
    assert [
        bytes(output[:length]) for output, length in zip(encoded_outputs, map(len, urlsafe), strict=True)
    ] == urlsafe

    assert hashcodecs.b64encode_batch_into(payloads, encoded_outputs, b'@#') == [len(value) for value in custom]
    assert [bytes(output[:length]) for output, length in zip(encoded_outputs, map(len, custom), strict=True)] == custom

    decoded_outputs = _BatchList([bytearray(len(value) + 2) for value in payloads])
    assert base64.standard_b64decode_batch_into(standard, decoded_outputs) == [len(value) for value in payloads]
    assert [bytes(output[:length]) for output, length in zip(decoded_outputs, map(len, payloads), strict=True)] == [
        bytes(value) for value in payloads
    ]

    assert base64.urlsafe_b64decode_batch_into(urlsafe, decoded_outputs) == [len(value) for value in payloads]
    assert [bytes(output[:length]) for output, length in zip(decoded_outputs, map(len, payloads), strict=True)] == [
        bytes(value) for value in payloads
    ]

    assert hashcodecs.b64decode_batch_into(custom, decoded_outputs, '@#', validate=True) == [
        len(value) for value in payloads
    ]
    assert [bytes(output[:length]) for output, length in zip(decoded_outputs, map(len, payloads), strict=True)] == [
        bytes(value) for value in payloads
    ]

    shared = bytearray(b'YWJj')
    assert base64.b64decode_batch_into([shared], [shared], validate=True) == [3]
    assert shared[:3] == b'abc'


def test_base64_batch_into_snapshots_cross_pair_aliases() -> None:
    shared = bytearray(b'abcd')
    encoded = bytearray(8)
    assert base64.b64encode_batch_into([b'xyz', shared], [shared, encoded]) == [4, 8]
    assert shared == b'eHl6'
    assert encoded == b'YWJjZA=='

    shared = bytearray(b'YWJj')
    decoded = bytearray(3)
    assert base64.b64decode_batch_into([b'ZGVm', shared], [shared, decoded], validate=True) == [3, 3]
    assert shared[:3] == b'def'
    assert decoded == b'abc'

    shared = bytearray(b'ZGVm')
    decoded = bytearray(3)
    observed: list[bytes] = []

    class AliasedString(str):
        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytearray:
            observed.append(bytes(shared))
            return shared

    assert base64.b64decode_batch_into([b'YWJj', AliasedString('ignored')], [shared, decoded], validate=True) == [
        3,
        3,
    ]
    assert observed == [b'ZGVm']
    assert shared[:3] == b'abc'
    assert decoded == b'def'


def test_base64_batch_into_snapshots_overlapping_memoryviews() -> None:
    empty_input = memoryview(bytearray(b'x'))[:0]
    empty_output = bytearray(b'!')
    assert base64.b64encode_batch_into([empty_input], [empty_output]) == [0]
    assert empty_output == b'!'

    encoded_storage = bytearray(b'abcd....')
    encoded_input = memoryview(encoded_storage)[:4]
    encoded_output = bytearray(8)
    assert base64.b64encode_batch_into([b'xyz', encoded_input], [encoded_storage, encoded_output]) == [4, 8]
    assert encoded_storage[:4] == b'eHl6'
    assert encoded_output == b'YWJjZA=='

    decoded_storage = bytearray(b'YWJj')
    decoded_input = memoryview(decoded_storage)
    decoded_output = bytearray(3)
    assert base64.b64decode_batch_into([b'ZGVm', decoded_input], [decoded_storage, decoded_output], validate=True) == [
        3,
        3,
    ]
    assert decoded_storage[:3] == b'def'
    assert decoded_output == b'abc'

    large_storage = bytearray((index * 29 + 7) & 0xFF for index in range(4096))
    large_input = memoryview(large_storage)
    expected_later = stdlib_base64.b64encode(large_storage)
    first_input = b'x' * 3072
    expected_first = stdlib_base64.b64encode(first_input)
    large_output = bytearray(len(expected_later))
    assert base64.b64encode_batch_into([first_input, large_input], [large_storage, large_output]) == [
        len(expected_first),
        len(expected_later),
    ]
    assert large_storage == expected_first
    assert large_output == expected_later


def test_base64_batch_into_reuses_output_ranges_for_generic_buffers() -> None:
    first_storage = bytearray(b'abcd')
    second_storage = bytearray(b'efgh....')
    first_output = bytearray(8)

    assert base64.b64encode_batch_into(
        [memoryview(first_storage), memoryview(second_storage)[:4]],
        [first_output, second_storage],
    ) == [8, 8]
    assert first_output == b'YWJjZA=='
    assert second_storage == b'ZWZnaA=='


def test_base64_batch_into_snapshots_large_exact_memoryview_owner() -> None:
    payload = b'a' * 50_000
    storage = bytearray(stdlib_base64.b64encode(payload))

    assert base64.b64decode_batch_into([memoryview(storage)], [storage], validate=True) == [len(payload)]
    assert storage[: len(payload)] == payload


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python-level buffer protocol support')
def test_base64_batch_snapshots_altchars_once() -> None:
    encode_altchars = _ChangingBuffer()
    assert base64.b64encode_batch([b'\xfb\xff', b'\xfb\xff'], encode_altchars) == [b'-_8=', b'-_8=']
    assert encode_altchars.calls == 1

    decode_altchars = _ChangingBuffer()
    assert base64.b64decode_batch([b'-_8=', b'-_8='], decode_altchars, validate=True) == [
        b'\xfb\xff',
        b'\xfb\xff',
    ]
    assert decode_altchars.calls == 1

    encode_outputs = [bytearray(4), bytearray(4)]
    encode_altchars = _ChangingBuffer()
    assert base64.b64encode_batch_into([b'\xfb\xff', b'\xfb\xff'], encode_outputs, encode_altchars) == [4, 4]
    assert encode_outputs == [b'-_8=', b'-_8=']
    assert encode_altchars.calls == 1

    decode_outputs = [bytearray(2), bytearray(2)]
    decode_altchars = _ChangingBuffer()
    assert base64.b64decode_batch_into([b'-_8=', b'-_8='], decode_outputs, decode_altchars, validate=True) == [2, 2]
    assert decode_outputs == [b'\xfb\xff', b'\xfb\xff']
    assert decode_altchars.calls == 1


def test_base64_batch_into_preflights_destinations_without_mutation() -> None:
    untouched = bytearray([0xA5] * 4)
    with pytest.raises(ValueError, match='same length'):
        base64.b64encode_batch_into([b'abc'], [])
    assert untouched == bytearray([0xA5] * 4)

    with pytest.raises(TypeError, match=r'outputs\[1\] must be a bytearray'):
        base64.b64encode_batch_into([b'abc', b'def'], [untouched, b'....'])  # type: ignore[list-item]
    assert untouched == bytearray([0xA5] * 4)

    with pytest.raises(ValueError, match='distinct bytearrays'):
        base64.b64encode_batch_into([b'abc', b'def'], [untouched, untouched])
    assert untouched == bytearray([0xA5] * 4)

    for items, outputs in (([b'abc'], (bytearray(4),)), ((b'abc',), [bytearray(4)])):
        with pytest.raises(TypeError):
            base64.b64encode_batch_into(items, outputs)  # type: ignore[arg-type]


def test_base64_batch_into_is_fail_fast_and_non_transactional() -> None:
    encoded_outputs = [bytearray([0xA5] * 4), bytearray([0xA5] * 3)]
    with pytest.raises(ValueError, match='requires 4 bytes'):
        base64.b64encode_batch_into([b'abc', b'def'], encoded_outputs)
    assert encoded_outputs[0] == b'YWJj'
    assert encoded_outputs[1] == bytearray([0xA5] * 3)

    decoded_outputs = [bytearray([0xA5] * 3), bytearray([0xA5] * 3)]
    with pytest.raises(binascii.Error):
        base64.b64decode_batch_into([b'YWJj', b'YWJ!'], decoded_outputs, validate=True)
    assert decoded_outputs[0] == b'abc'

    encoded_outputs = [bytearray([0xA5] * 4), bytearray([0xA5] * 4)]
    with pytest.raises(TypeError, match='bytes-like object'):
        base64.b64encode_batch_into([b'abc', object()], encoded_outputs)
    assert encoded_outputs[0] == b'YWJj'
    assert encoded_outputs[1] == bytearray([0xA5] * 4)

    released = memoryview(b'abc')
    released.release()
    encoded_outputs = [bytearray([0xA5] * 4), bytearray([0xA5] * 4)]
    with pytest.raises(ValueError, match='released memoryview'):
        base64.b64encode_batch_into([b'abc', released], encoded_outputs)
    assert encoded_outputs[0] == b'YWJj'
    assert encoded_outputs[1] == bytearray([0xA5] * 4)

    released = memoryview(b'YWJj')
    released.release()
    decoded_outputs = [bytearray([0xA5] * 3), bytearray([0xA5] * 3)]
    with pytest.raises(ValueError, match='released memoryview'):
        base64.b64decode_batch_into([b'YWJj', released], decoded_outputs, validate=True)
    assert decoded_outputs[0] == b'abc'
    assert decoded_outputs[1] == bytearray([0xA5] * 3)


@pytest.mark.parametrize('failure_index', [0, 1, 2])
def test_base64_batch_decode_is_fail_fast(failure_index: int) -> None:
    encoded = [b'YQ==', b'Yg==', b'Yw==']
    encoded[failure_index] = b'not base64!'
    with pytest.raises(binascii.Error):
        base64.b64decode_batch(encoded, validate=True)


def test_base64_batch_lenient_mode_and_noncontiguous_decode() -> None:
    values = [b'Y W\nJj', b'YWJj====']
    assert base64.b64decode_batch(values) == [b'abc', b'abc']

    trailing_data = b'AA==anything after padding'
    try:
        expected = base64.b64decode(trailing_data)
    except Exception as error:
        with pytest.raises(type(error)):
            base64.b64decode_batch([trailing_data])
    else:
        assert base64.b64decode_batch([trailing_data]) == [expected]

    assert base64.b64decode_batch([memoryview(b'YxWxJxjx')[::2]], validate=True) == [b'abc']


def test_base64_batch_rejects_invalid_inputs() -> None:
    for outer in ((b'abc',), iter([b'abc']), b'abc'):
        with pytest.raises(TypeError):
            base64.b64encode_batch(outer)  # type: ignore[arg-type]
        with pytest.raises(TypeError):
            base64.b64decode_batch(outer)  # type: ignore[arg-type]

    with pytest.raises(TypeError):
        base64.b64encode_batch([object()])  # type: ignore[list-item]
    with pytest.raises(TypeError):
        base64.b64decode_batch([[65, 66]])  # type: ignore[list-item]
    with pytest.raises(ValueError, match='only ASCII'):
        base64.b64decode_batch(['\u2603'])
    with pytest.raises(BufferError):
        base64.b64encode_batch([memoryview(b'abcdef')[::2]])
    with pytest.raises(ALTCHARS_ERROR):
        base64.b64encode_batch([], b'_')
    with pytest.raises(ALTCHARS_ERROR):
        base64.b64decode_batch([], b'___')


def test_base64_batch_exports_docstrings_and_signatures() -> None:
    names = {
        'b64decode_batch',
        'b64decode_batch_into',
        'b64encode_batch',
        'b64encode_batch_into',
        'standard_b64decode_batch',
        'standard_b64decode_batch_into',
        'standard_b64encode_batch',
        'standard_b64encode_batch_into',
        'urlsafe_b64decode_batch',
        'urlsafe_b64decode_batch_into',
        'urlsafe_b64encode_batch',
        'urlsafe_b64encode_batch_into',
    }
    assert names <= set(base64.__all__)
    assert names <= set(hashcodecs.__all__)
    assert all(inspect.isbuiltin(getattr(base64, name)) for name in names)
    assert base64.b64encode_batch.__doc__
    assert base64.b64decode_batch.__doc__
    assert base64.b64encode_batch_into.__doc__
    assert base64.b64decode_batch_into.__doc__
    assert str(inspect.signature(base64.b64encode_batch)) == '(items, altchars=None)'
    assert str(inspect.signature(base64.b64decode_batch)) == '(items, altchars=None, validate=False)'
    assert str(inspect.signature(base64.b64encode_batch_into)) == '(items, outputs, altchars=None)'
    assert str(inspect.signature(base64.b64decode_batch_into)) == '(items, outputs, altchars=None, validate=False)'


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
