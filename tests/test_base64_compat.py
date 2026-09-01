import ast
import base64 as stdlib_base64
import binascii
import inspect
import sys
import warnings
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

import hashcodecs
import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)
FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()
ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError
BASE64_DETACH_THRESHOLD = 256 * 1024
GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


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
    assert base64.b64decode(b'YWJj', padded=False) == b'abc'
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
    assert base64.b64encode(b'\xfb\xff', b'/+') == b'/+8='
    assert base64.b64encode(b'\xfb\xff', b'+/') == b'+/8='
    assert base64.b64decode(b'+/8=', b'+/', validate=True) == b'\xfb\xff'


def test_unpadded_decoding_uses_direct_tail_path() -> None:
    for length in range(1025):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        standard = stdlib_base64.b64encode(payload).rstrip(b'=')
        urlsafe = stdlib_base64.urlsafe_b64encode(payload).rstrip(b'=')
        assert base64.b64decode(standard, padded=False, validate=True) == payload
        assert base64.b64decode(urlsafe, b'-_', padded=False, validate=True) == payload

        output = bytearray([0xA5] * (length + 16))
        assert base64.b64decode_into(standard, output, padded=False, validate=True) == length
        assert output[:length] == payload
        assert output[length:] == bytes([0xA5] * 16)

    # '=' is a valid custom-alphabet data character, not padding, after it is
    # translated to the standard alphabet.
    assert base64.b64decode(b'=w', b'=_', padded=False, validate=True) == b'\xfb'


def test_unpadded_decode_into_rejects_invalid_tails_without_writing_them() -> None:
    for encoded in (b'A!', b'AA!', b'A=', b'AA='):
        output = bytearray([0xA5] * 8)
        with pytest.raises(binascii.Error):
            base64.b64decode_into(encoded, output, padded=False, validate=True)
        assert output == bytes([0xA5] * 8)


def _outcome(function: Callable[..., bytes], value: bytes | bytearray, altchars: bytes | None, validate: bool) -> Any:
    try:
        with warnings.catch_warnings():
            warnings.simplefilter('ignore')
            return function(value, altchars, validate=validate)
    except Exception as error:
        return type(error)


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


def test_base64_functions_are_native_and_keep_public_metadata() -> None:
    for name in (
        'b64decode',
        'b64decode_batch',
        'b64decode_batch_into',
        'b64decode_into',
        'b64encode',
        'b64encode_batch',
        'b64encode_batch_into',
        'b64encode_into',
        'standard_b64decode',
        'standard_b64decode_into',
        'standard_b64encode',
        'standard_b64encode_into',
        'urlsafe_b64decode',
        'urlsafe_b64decode_into',
        'urlsafe_b64encode',
        'urlsafe_b64encode_into',
    ):
        function = getattr(base64, name)
        assert inspect.isbuiltin(function)
        assert function.__module__ == 'hashcodecs.base64'
        assert function.__doc__


def test_base64_binding_schema_exports_stable_signatures() -> None:
    padded_default = 'False' if PYTHON_315 else 'True'
    expected = {
        'b64decode': "(s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, "
        "ignorechars=['NOT SPECIFIED'], canonical=False)",
        'b64decode_batch': '(items, altchars=None, validate=False)',
        'b64decode_batch_into': '(items, outputs, altchars=None, validate=False)',
        'b64decode_into': "(s, output, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, "
        "ignorechars=['NOT SPECIFIED'], canonical=False)",
        'b64encode': '(s, altchars=None, *, padded=True, wrapcol=0)',
        'b64encode_batch': '(items, altchars=None)',
        'b64encode_batch_into': '(items, outputs, altchars=None)',
        'b64encode_into': '(s, output, altchars=None, *, padded=True, wrapcol=0)',
        'standard_b64decode': '(s)',
        'standard_b64decode_batch': '(items)',
        'standard_b64decode_batch_into': '(items, outputs)',
        'standard_b64decode_into': '(s, output)',
        'standard_b64encode': '(s)',
        'standard_b64encode_batch': '(items)',
        'standard_b64encode_batch_into': '(items, outputs)',
        'standard_b64encode_into': '(s, output)',
        'urlsafe_b64decode': f'(s, *, padded={padded_default})',
        'urlsafe_b64decode_batch': '(items)',
        'urlsafe_b64decode_batch_into': '(items, outputs)',
        'urlsafe_b64decode_into': f'(s, output, *, padded={padded_default})',
        'urlsafe_b64encode': '(s, *, padded=True)',
        'urlsafe_b64encode_batch': '(items)',
        'urlsafe_b64encode_batch_into': '(items, outputs)',
        'urlsafe_b64encode_into': '(s, output, *, padded=True)',
    }
    assert set(expected) == set(base64.__all__)
    assert {name: str(inspect.signature(getattr(base64, name))) for name in expected} == expected


def test_base64_binding_schema_exports_complete_typed_documentation() -> None:
    stub = Path(hashcodecs.__file__).with_name('_hashcodecs.pyi')
    declarations = ast.parse(stub.read_text(encoding='utf-8'), filename=str(stub))
    expected = {
        node.name: ast.get_docstring(node, clean=True)
        for node in declarations.body
        if isinstance(node, ast.FunctionDef) and 'b64' in node.name
    }

    assert set(expected) == set(base64.__all__)
    assert {name: getattr(base64, name).__doc__ for name in expected} == expected


def test_base64_binding_schema_drives_argument_errors() -> None:
    with pytest.raises(TypeError, match=r"standard_b64encode\(\) missing required argument 's'"):
        base64.standard_b64encode()
    with pytest.raises(TypeError, match=r'urlsafe_b64encode\(\) takes at most 1 positional arguments'):
        base64.urlsafe_b64encode(b'', True)
    with pytest.raises(TypeError, match=r"standard_b64decode\(\) got an unexpected keyword argument 'unknown'"):
        base64.standard_b64decode(b'', unknown=True)
    with pytest.raises(TypeError, match=r"b64encode\(\) got multiple values for argument 's'"):
        base64.b64encode(b'', s=b'')


def test_b64decode_into_signature_does_not_advertise_sentinel_defaults_as_none() -> None:
    parameters = inspect.signature(base64.b64decode_into).parameters
    assert parameters['validate'].default is not None
    assert parameters['ignorechars'].default is not None
    with pytest.raises(TypeError):
        base64.b64decode_into(b'YWJj', bytearray(3), ignorechars=None)

    omitted = bytearray(3)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'YWJj!', omitted, ignorechars=b'')
    explicit_none = bytearray(3)
    assert base64.b64decode_into(b'YWJj!', explicit_none, validate=None, ignorechars=b'') == 3
    assert explicit_none == b'abc'


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


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_b64decode_signature_matches_cpython() -> None:
    assert str(inspect.signature(base64.b64decode)) == str(inspect.signature(stdlib_base64.b64decode))
