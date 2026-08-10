import base64 as stdlib_base64
import binascii
import inspect
import sys
import threading
import warnings
from collections.abc import Callable
from time import sleep
from typing import Any

import hashcodecs
import hashcodecs.base64 as base64
import pytest

PYTHON_315 = sys.version_info >= (3, 15)
ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError


@pytest.mark.parametrize('payload', [b'', b'a', b'ab', b'abc', bytes(range(256))])
def test_base64_compatibility(payload: bytes) -> None:
    encoded = stdlib_base64.b64encode(payload)

    assert base64.b64encode(payload) == encoded
    assert base64.standard_b64encode(payload) == encoded
    assert base64.b64decode(encoded) == payload
    assert base64.b64decode(encoded, validate=True) == payload
    assert base64.standard_b64decode(encoded.decode('ascii')) == payload


def test_base64_variants_and_lenient_mode() -> None:
    assert base64.urlsafe_b64encode(b'\xfb\xff') == b'-_8='
    assert base64.urlsafe_b64decode(b'-_8=') == b'\xfb\xff'
    assert base64.b64decode(b'-_8=', b'-_', validate=True) == b'\xfb\xff'
    assert base64.b64encode(bytearray(b'abc')) == b'YWJj'
    assert base64.b64decode(bytearray(b'YWJj'), validate=True) == b'abc'
    assert base64.b64encode(b'\xfb\xff', bytearray(b'@#')) == b'@#8='
    assert base64.b64encode(memoryview(b'abc')) == b'YWJj'
    assert base64.b64decode(b'Y W\nJj', validate=False) == b'abc'
    assert base64.b64decode(b'YWJj====', validate=False) == b'abc'
    trailing_data = b'AA==anything after padding'
    assert _outcome(base64.b64decode, trailing_data, None, False) == _outcome(
        stdlib_base64.b64decode, trailing_data, None, False
    )
    assert base64.b64decode(b'AA=\n=') == b'\x00'
    with warnings.catch_warnings():
        warnings.simplefilter('ignore')
        assert base64.b64decode(b'++8=', b'-_', validate=True) == b'\xfb\xef'
        assert base64.b64decode(b'//8=', b'-_', validate=True) == b'\xff\xff'
        assert base64.b64decode(b'+-8=', b'-_', validate=True) == stdlib_base64.b64decode(
            b'+-8=', b'-_', validate=True
        )
    assert base64.b64decode(b'-_8=', '-_', validate=True) == b'\xfb\xff'
    assert base64.b64decode(b'++8=', b'++') == stdlib_base64.b64decode(b'++8=', b'++')
    assert base64.b64decode(b'++8=', b'++', validate=True) == stdlib_base64.b64decode(b'++8=', b'++', validate=True)
    assert base64.b64encode(b'\xfb\xff', b'@#') == b'@#8='


def test_base64_into_variants_and_errors() -> None:
    encoded = bytearray([0xA5] * 12)
    assert base64.b64encode_into(b'abc', encoded) == 4
    assert encoded[:4] == b'YWJj'
    assert encoded[4:] == bytearray([0xA5] * 8)
    assert base64.b64encode_into(bytearray(b'abc'), encoded) == 4
    assert base64.standard_b64encode_into(b'abc', encoded) == 4
    assert hashcodecs.b64encode_into(b'\xfb\xff', encoded, b'@#') == 4
    assert encoded[:4] == b'@#8='
    assert base64.urlsafe_b64encode_into(b'\xfb\xff', encoded) == 4
    assert encoded[:4] == b'-_8='

    decoded = bytearray([0xA5] * 8)
    assert base64.b64decode_into(b'Y W\nJj', decoded) == 3
    assert decoded[:3] == b'abc'
    assert decoded[3:] == bytearray([0xA5] * 5)
    assert base64.standard_b64decode_into(b'YWJj', decoded) == 3
    assert hashcodecs.b64decode_into(b'@#8=', decoded, b'@#', validate=True) == 2
    assert decoded[:2] == b'\xfb\xff'
    assert base64.urlsafe_b64decode_into(b'-_8=', decoded) == 2
    assert decoded[:2] == b'\xfb\xff'

    with pytest.raises(ValueError, match='requires 4 bytes'):
        base64.b64encode_into(b'abc', bytearray(3))
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'YWJj', bytearray(2), validate=True)
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'Y W\nJj', bytearray(2))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'YWJj!', bytearray(8), validate=True)
    with pytest.raises(TypeError):
        base64.b64encode_into(b'abc', b'....')  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        base64.b64decode_into(b'YWJj', memoryview(bytearray(3)))  # type: ignore[arg-type]


def test_base64_into_handles_aliases_and_every_short_length() -> None:
    shared = bytearray(8)
    shared[:3] = b'abc'
    assert base64.b64encode_into(memoryview(shared)[:3], shared) == 4
    assert shared[:4] == b'YWJj'
    assert base64.b64decode_into(memoryview(shared)[:4], shared, validate=True) == 3
    assert shared[:3] == b'abc'

    shared = bytearray(b'YWJj')
    assert base64.b64decode_into(shared, shared, validate=True) == 3
    assert shared[:3] == b'abc'

    for length in range(1025):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        standard = stdlib_base64.b64encode(payload)
        urlsafe = stdlib_base64.urlsafe_b64encode(payload)
        encoded = bytearray(len(standard) + 1)
        decoded = bytearray(length + 1)

        written = base64.b64encode_into(payload, encoded)
        assert written == len(standard)
        assert encoded[:written] == standard
        assert encoded[written] == 0
        written = base64.b64decode_into(standard, decoded, validate=True)
        assert written == length
        assert decoded[:written] == payload
        assert decoded[written] == 0

        written = base64.b64encode_into(payload, encoded, b'-_')
        assert written == len(urlsafe)
        assert encoded[:written] == urlsafe
        assert encoded[written] == 0
        written = base64.b64decode_into(urlsafe, decoded, b'-_', validate=True)
        assert written == length
        assert decoded[:written] == payload
        assert decoded[written] == 0


@pytest.mark.parametrize(
    ('value', 'kwargs', 'exception'),
    [
        (b'YWJj!', {'validate': True}, binascii.Error),
        (b'abc', {}, binascii.Error),
        ('\u2603', {}, ValueError),
        (b'abc', {'altchars': b'x'}, ALTCHARS_ERROR),
        ([65, 66], {}, TypeError),
    ],
)
def test_base64_invalid_inputs(value: object, kwargs: dict[str, object], exception: type[Exception]) -> None:
    with pytest.raises(exception):
        base64.b64decode(value, **kwargs)  # type: ignore[arg-type]


def test_encode_requires_contiguous_buffers() -> None:
    noncontiguous = memoryview(b'abcdef')[::2]
    with pytest.raises(BufferError):
        base64.b64encode(noncontiguous)
    with pytest.raises(BufferError):
        base64.b64encode(b'abc', memoryview(b'_-x_')[::2])
    with pytest.raises(ALTCHARS_ERROR):
        base64.b64encode(b'abc', b'_')


def _outcome(function: Callable[..., bytes], value: bytes | bytearray, altchars: bytes | None, validate: bool) -> Any:
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            return function(value, altchars, validate=validate)
    except Exception as error:
        return type(error)


def _into_outcome(value: bytes | bytearray, altchars: bytes | None, validate: bool) -> Any:
    output = bytearray(len(value))
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            written = base64.b64decode_into(value, output, altchars, validate=validate)
        return bytes(output[:written])
    except Exception as error:
        return type(error)


@pytest.mark.parametrize(
    'value',
    [
        b'',
        b'A',
        b'AA',
        b'AAA',
        b'AAAA',
        b'AA=',
        b'YQ=',
        b'YWI==',
        b'YWJj====',
        b'AAAA=AAA',
        b'AA==AA',
        b'=AAA',
        b'====',
        b'A===',
        b'AA===',
        b'AAA===',
        b'AAAA===',
        b'AA==junk',
        b'AA==!!',
        b'YW=Jj',
        b'YWJ=j',
        b'A=AAA',
        b'A==AAA',
        b'AA=A',
        b'AA==A',
        b'AAA=A',
        b'AAAA=A',
        b'AA=!!=',
        b'AA=! =',
        b'AA=Z=',
        b'AA=Z==',
        b'AA\n=',
        b'AA=\n=',
        b'++8=',
        b'--8=',
        b'//8=',
        b'__8=',
        b'+-8=',
        b'/_8=',
        b'Y W\nJj',
    ],
)
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#', b'++', b'A_'])
@pytest.mark.parametrize('validate', [False, True])
def test_decode_edge_cases_match_cpython(value: bytes, altchars: bytes | None, validate: bool) -> None:
    expected = _outcome(stdlib_base64.b64decode, value, altchars, validate)
    actual = _outcome(base64.b64decode, value, altchars, validate)
    assert actual == expected
    assert _into_outcome(value, altchars, validate) == expected
    mutable = bytearray(value)
    assert _outcome(base64.b64decode, mutable, altchars, validate) == expected
    assert _into_outcome(mutable, altchars, validate) == expected


def test_all_short_payload_lengths_match_cpython() -> None:
    for length in range(1025):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        standard = stdlib_base64.b64encode(payload)
        urlsafe = stdlib_base64.urlsafe_b64encode(payload)
        assert base64.b64encode(payload) == standard
        assert base64.b64decode(standard) == payload
        assert base64.urlsafe_b64encode(payload) == urlsafe
        assert base64.urlsafe_b64decode(urlsafe) == payload


def test_large_base64_calls_cross_the_gil_release_threshold() -> None:
    payload = bytes(range(256)) * 257
    encoded = stdlib_base64.b64encode(payload)
    assert base64.b64encode(payload) == encoded
    assert base64.b64decode(encoded) == payload


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_encode_options_match_cpython() -> None:
    for length in range(129):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        for altchars in (None, b'-_', b'@#'):
            for padded in (False, True):
                for wrapcol in (0, 1, 3, 4, 5, 7, 8, 11, 12, 76, 80, 1000):
                    expected = stdlib_base64.b64encode(
                        payload,
                        altchars,
                        padded=padded,
                        wrapcol=wrapcol,
                    )
                    assert base64.b64encode(payload, altchars, padded=padded, wrapcol=wrapcol) == expected
                    output = bytearray([0xA5] * (len(expected) + 1))
                    written = base64.b64encode_into(
                        payload,
                        output,
                        altchars,
                        padded=padded,
                        wrapcol=wrapcol,
                    )
                    assert bytes(output[:written]) == expected
                    assert output[written] == 0xA5


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_encode_option_errors_match_cpython() -> None:
    for kwargs in ({'wrapcol': -1}, {'wrapcol': 1.5}, {'wrapcol': None}, {'wrapcol': 2**1000}):
        expected = _keyword_outcome(stdlib_base64.b64encode, b'abc', kwargs)
        assert _keyword_outcome(base64.b64encode, b'abc', kwargs) == expected
    assert base64.b64encode(b'a', padded=[]) == stdlib_base64.b64encode(b'a', padded=[])


def _keyword_outcome(function: Callable[..., bytes], value: bytes, kwargs: dict[str, object]) -> Any:
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            return function(value, **kwargs)
    except Exception as error:
        return type(error)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    ('value', 'altchars', 'kwargs'),
    [
        (b'AA', None, {'padded': False}),
        (b'AAA', None, {'padded': False, 'validate': True}),
        (b'AA=', None, {'padded': False}),
        (b'AA=', None, {'padded': False, 'validate': True}),
        (b'Y WJj', None, {'ignorechars': b' '}),
        (b'Y WJj', None, {'ignorechars': b' ', 'validate': False}),
        (b'Y WJj', None, {'ignorechars': b''}),
        (b'AB==', None, {'canonical': True}),
        (b'AA==', None, {'canonical': True}),
        (b'AP', None, {'padded': False, 'canonical': True}),
        (b'@#8', b'@#', {'padded': False, 'ignorechars': b''}),
        (b'++8=', b'-_', {'ignorechars': b''}),
    ],
)
def test_python_315_decode_options_match_cpython(
    value: bytes,
    altchars: bytes | None,
    kwargs: dict[str, object],
) -> None:
    expected = _decode_keyword_outcome(stdlib_base64.b64decode, value, altchars, kwargs)
    actual = _decode_keyword_outcome(base64.b64decode, value, altchars, kwargs)
    assert actual == expected

    output = bytearray(len(value) + 1)
    try:
        written = base64.b64decode_into(value, output, altchars, **kwargs)
        into = bytes(output[:written])
    except Exception as error:
        into = type(error)
    assert into == expected


def _decode_keyword_outcome(
    function: Callable[..., bytes],
    value: bytes,
    altchars: bytes | None,
    kwargs: dict[str, object],
) -> Any:
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            return function(value, altchars, **kwargs)
    except Exception as error:
        return type(error)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_ignorechars_and_altchar_warnings() -> None:
    assert base64.b64decode(b'Y WJj', ignorechars=memoryview(b' ')) == b'abc'
    with pytest.raises(TypeError):
        base64.b64decode(b'YWJj', ignorechars=None)
    with pytest.warns(FutureWarning, match="invalid character '\\+'"):
        assert base64.b64decode(b'++8=', b'-_') == b'\xfb\xef'
    with pytest.warns(DeprecationWarning, match="invalid character '/'"):
        assert base64.b64decode(b'//8=', b'-_', validate=True) == b'\xff\xff'


def test_urlsafe_padding_options_follow_the_running_cpython() -> None:
    assert base64.urlsafe_b64encode(b'\xfb\xff', padded=False) == b'-_8'
    assert base64.urlsafe_b64decode(b'-_8', padded=False) == b'\xfb\xff'
    assert base64.urlsafe_b64decode(b'-_8=', padded=True) == b'\xfb\xff'

    encoded = bytearray(4)
    assert base64.urlsafe_b64encode_into(b'\xfb\xff', encoded, padded=False) == 3
    assert encoded[:3] == b'-_8'
    decoded = bytearray(2)
    assert base64.urlsafe_b64decode_into(b'-_8', decoded, padded=False) == 2
    assert decoded == b'\xfb\xff'
    assert base64.urlsafe_b64decode_into(b'-_8=', decoded, padded=True) == 2
    assert decoded == b'\xfb\xff'

    if PYTHON_315:
        assert base64.urlsafe_b64decode(b'-_8') == b'\xfb\xff'
    else:
        with pytest.raises(binascii.Error):
            base64.urlsafe_b64decode(b'-_8')


def test_python_315_decode_options_are_backported() -> None:
    assert base64.b64decode(b'Y WJj', ignorechars=b' ') == b'abc'
    assert base64.b64decode(b'@#8', b'@#', padded=False, ignorechars=b'') == b'\xfb\xff'
    assert base64.b64decode(b'AA', padded=False, canonical=True) == b'\x00'
    with pytest.raises(binascii.Error):
        base64.b64decode(b'AB', padded=False, canonical=True)


class _BatchList(list[object]):
    pass


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


@pytest.mark.parametrize('failure_index', [0, 1, 2])
def test_base64_batch_decode_is_fail_fast(failure_index: int) -> None:
    encoded = [b'YQ==', b'Yg==', b'Yw==']
    encoded[failure_index] = b'not base64!'
    with pytest.raises(binascii.Error):
        base64.b64decode_batch(encoded, validate=True)


def test_base64_batch_lenient_mode_and_noncontiguous_decode() -> None:
    values = [b'Y W\nJj', b'YWJj====', b'AA==anything after padding']
    expected = [stdlib_base64.b64decode(value) for value in values]
    assert base64.b64decode_batch(values) == expected
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
    with pytest.raises(AssertionError):
        base64.b64encode_batch([], b'_')
    with pytest.raises(AssertionError):
        base64.b64decode_batch([], b'___')


def test_base64_batch_exports_docstrings_and_signatures() -> None:
    names = {
        'b64decode_batch',
        'b64encode_batch',
        'standard_b64decode_batch',
        'standard_b64encode_batch',
        'urlsafe_b64decode_batch',
        'urlsafe_b64encode_batch',
    }
    assert names <= set(base64.__all__)
    assert names <= set(hashcodecs.__all__)
    assert base64.b64encode_batch.__doc__
    assert base64.b64decode_batch.__doc__
    assert str(inspect.signature(base64.b64encode_batch)) == '(items, altchars=None)'
    assert str(inspect.signature(base64.b64decode_batch)) == '(items, altchars=None, validate=False)'


def _assert_batch_releases_the_gil(operation: Callable[[], list[bytes]], expected: list[bytes]) -> None:
    ready = threading.Event()
    progressed = threading.Event()

    def delayed_progress() -> None:
        ready.set()
        sleep(0.001)
        progressed.set()

    worker = threading.Thread(target=delayed_progress)
    worker.start()
    assert ready.wait(timeout=1)
    result = operation()
    progressed_during_call = progressed.is_set()
    worker.join(timeout=1)

    assert not worker.is_alive()
    assert progressed_during_call
    assert result == expected


def test_large_base64_batch_crosses_the_gil_release_threshold() -> None:
    payload = bytes(range(256)) * 256
    encoded_item = stdlib_base64.b64encode(payload)
    payloads = [payload] * 1024
    encoded = [encoded_item] * 1024

    _assert_batch_releases_the_gil(lambda: base64.b64encode_batch(payloads), encoded)
    _assert_batch_releases_the_gil(lambda: base64.b64decode_batch(encoded, validate=True), payloads)
