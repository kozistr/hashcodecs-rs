import base64 as stdlib_base64
import binascii
import inspect
import sys
import warnings
from collections.abc import Callable, Mapping

import pytest
from base64_compat_harness import Observation, observe_call

import hashcodecs.base64 as base64

stdlib_b64encode: Callable[..., bytes] = stdlib_base64.b64encode

stdlib_b64decode: Callable[..., bytes] = stdlib_base64.b64decode

dynamic_b64encode: Callable[..., bytes] = base64.b64encode

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into

PYTHON_315 = sys.version_info >= (3, 15)

BASE64_ALPHABET = getattr(
    binascii, 'BASE64_ALPHABET', b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_encode_options_match_cpython() -> None:
    for length in range(129):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        for altchars in (None, b'-_', b'@#'):
            for padded in (False, True):
                for wrapcol in (0, 1, 3, 4, 5, 7, 8, 11, 12, 76, 80, 1000):
                    expected = stdlib_b64encode(
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
def test_python_315_constructed_alphabets_match_cpython() -> None:
    class AlphabetBuffer:
        def __buffer__(self, flags: int) -> memoryview:
            return memoryview(b'Z' * 64)

    class Altchars(bytes):
        alphabet: object

        def __radd__(self, other: object) -> object:
            return self.alphabet

    for alphabet in (b'Z' * 64, bytearray(b'Z' * 64), AlphabetBuffer()):
        altchars = Altchars(b'-_')
        altchars.alphabet = alphabet
        expected = stdlib_base64.b64encode(b'abc', altchars)
        assert expected == b'ZZZZ'
        assert base64.b64encode(b'abc', altchars) == expected

    payload = bytes(range(256))
    altchars.alphabet = BASE64_ALPHABET[::-1]
    assert base64.b64encode(payload, altchars) == stdlib_base64.b64encode(payload, altchars)

    encoded = b'YWJj'
    expected = stdlib_b64decode(encoded, altchars, ignorechars=b'')
    assert expected == b'\x9e\x9d\x9c'
    assert base64.b64decode(encoded, altchars, ignorechars=b'') == expected
    output = bytearray(len(expected))
    assert base64.b64decode_into(encoded, output, altchars, ignorechars=b'') == len(expected)
    assert output == expected

    altchars.alphabet = b'=' + BASE64_ALPHABET[1:]
    expected = stdlib_b64decode(b'BB==', altchars, ignorechars=b'')
    assert expected == b'\x04'
    assert base64.b64decode(b'BB==', altchars, ignorechars=b'') == expected
    output = bytearray(len(expected))
    assert base64.b64decode_into(b'BB==', output, altchars, ignorechars=b'') == len(expected)
    assert output == expected


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_encode_option_errors_match_cpython() -> None:
    for kwargs in ({'wrapcol': -1}, {'wrapcol': 1.5}, {'wrapcol': None}, {'wrapcol': 2**1000}):
        expected = _keyword_outcome(stdlib_base64.b64encode, b'abc', kwargs)
        assert _keyword_outcome(base64.b64encode, b'abc', kwargs) == expected
    assert dynamic_b64encode(b'a', padded=[]) == stdlib_b64encode(b'a', padded=[])


def _keyword_outcome(function: Callable[..., bytes], value: bytes, kwargs: Mapping[str, object]) -> Observation:
    return observe_call(lambda: function(value, **kwargs))


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
        (b'AB==AA', None, {'canonical': True}),
        (b'AA==', None, {'canonical': True}),
        (b'AP', None, {'padded': False, 'canonical': True}),
        (b'@#8', b'@#', {'padded': False, 'ignorechars': b''}),
        (b'++8=', b'-_', {'ignorechars': b''}),
        (b'AA==', None, {'ignorechars': b'!$%&'}),
        (b'AAA=', None, {'ignorechars': b'!$%&'}),
        (b'AA==A', None, {'ignorechars': b'!$%&'}),
        (b'AA~=', None, {'ignorechars': b'!$%&'}),
        (b'A===', None, {'ignorechars': b'!$%&'}),
        (b'AA=', None, {'ignorechars': b'!$%&'}),
        (b'AB==', None, {'ignorechars': b'!$%&', 'canonical': True}),
        (b'AA==', None, {'padded': False, 'ignorechars': b'!$%&'}),
        (b'AA==!', None, {'ignorechars': b'!'}),
        (b'AAA=', None, {'ignorechars': b'!'}),
        (b'AA==A', None, {'ignorechars': b'!'}),
        (b'AA==~', None, {'ignorechars': b'!'}),
        (b'A===', None, {'ignorechars': b'!'}),
        (b'AA=', None, {'ignorechars': b'!'}),
        (b'AB==', None, {'ignorechars': b'!', 'canonical': True}),
        (b'AA==', None, {'padded': False, 'ignorechars': b'!'}),
        (b'=', None, {'ignorechars': b'='}),
        (b'AA===', None, {'ignorechars': b'='}),
        (b'AAAA=', None, {'ignorechars': b'='}),
        (b'=AA==', None, {'ignorechars': b'='}),
        (b'A=A=', None, {'ignorechars': b'='}),
        (b'A', None, {'validate': False, 'ignorechars': b'!$%&'}),
        (b'AA', None, {'validate': False, 'ignorechars': b'!$%&'}),
        (b'AB==', None, {'validate': False, 'ignorechars': b'!$%&', 'canonical': True}),
        (b'AA==AAAA', None, {'validate': False, 'ignorechars': b'!$%&'}),
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

    def decode_into() -> bytes:
        written = dynamic_b64decode_into(value, output, altchars, **kwargs)
        return bytes(output[:written])

    assert observe_call(decode_into) == expected


def _decode_keyword_outcome(
    function: Callable[..., bytes],
    value: bytes,
    altchars: bytes | None,
    kwargs: dict[str, object],
) -> Observation:
    return observe_call(lambda: function(value, altchars, **kwargs))


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_ignorechars_and_altchar_warnings() -> None:
    assert base64.b64decode(b'Y WJj', ignorechars=memoryview(b' ')) == b'abc'
    with pytest.raises(TypeError):
        dynamic_b64decode(b'YWJj', ignorechars=None)
    output = bytearray(2)
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter('always')
        assert base64.b64decode(b'-_8=', b'-_', validate=True) == b'\xfb\xff'
        assert base64.b64decode(b'-_8', b'-_', validate=True, padded=False) == b'\xfb\xff'
        assert base64.b64decode_into(b'-_8=', output, b'-_', validate=True) == 2
        assert output == b'\xfb\xff'
        assert not caught
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter('always')
        with pytest.raises(binascii.Error):
            base64.b64decode(b'/', b'++', validate=True)
        assert not caught
    with pytest.warns(FutureWarning, match="invalid character '\\+'"):
        assert base64.b64decode(b'++8=', b'-_') == b'\xfb\xef'
    with pytest.warns(DeprecationWarning, match="invalid character '/'"):
        assert base64.b64decode(b'//8=', b'-_', validate=True) == b'\xff\xff'
    with pytest.warns(DeprecationWarning, match="invalid character '/'"):
        assert base64.b64decode_into(b'//8=', output, b'-_', validate=True) == 2
    assert output == b'\xff\xff'


def test_urlsafe_padding_options_follow_the_running_cpython() -> None:
    expected_default = not PYTHON_315
    assert inspect.signature(base64.urlsafe_b64decode).parameters['padded'].default is expected_default
    assert inspect.signature(base64.urlsafe_b64decode_into).parameters['padded'].default is expected_default
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


@pytest.mark.skipif(PYTHON_315, reason='exercises the backported pre-3.15 error path')
def test_legacy_decode_error_preserves_binascii_lookup_failures(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(binascii, 'Error', None)
    with pytest.raises(TypeError):
        base64.b64decode(b'A!', padded=False, validate=False, ignorechars=b'!')


@pytest.mark.skipif(PYTHON_315, reason='exercises the backported pre-3.15 error path')
def test_legacy_decode_rejects_data_after_unpadded_padding_errors() -> None:
    encoded = b'=A'
    with pytest.raises(binascii.Error):
        base64.b64decode(encoded, padded=False, validate=False, ignorechars=b'!')
    output = bytearray([0xA5] * 4)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(encoded, output, padded=False, validate=False, ignorechars=b'!')
    assert output == bytearray([0xA5] * 4)
