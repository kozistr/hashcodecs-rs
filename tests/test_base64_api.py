import ast
import base64 as stdlib_base64
import binascii
import inspect
import sys
import warnings
from collections.abc import Callable
from pathlib import Path

import pytest
from base64_compat_harness import Observation, observe_call

import hashcodecs
import hashcodecs.base64 as base64

dynamic_b64encode: Callable[..., bytes] = base64.b64encode

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into

dynamic_standard_b64encode: Callable[..., bytes] = base64.standard_b64encode

dynamic_urlsafe_b64encode: Callable[..., bytes] = base64.urlsafe_b64encode

dynamic_standard_b64decode: Callable[..., bytes] = base64.standard_b64decode

PYTHON_315 = sys.version_info >= (3, 15)

ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError


@pytest.mark.parametrize(
    'length', [0, 1, 2, 3, 11, 12, 13, 23, 24, 25, 47, 48, 49, 63, 64, 65, 95, 96, 97, 255, 256, 257, 1023, 1024, 1025]
)
@pytest.mark.parametrize('urlsafe', [False, True], ids=['standard', 'urlsafe'])
def test_public_apis_match_cpython_at_block_boundaries(length: int, urlsafe: bool) -> None:
    payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
    encode = base64.urlsafe_b64encode if urlsafe else base64.b64encode
    decode = base64.urlsafe_b64decode if urlsafe else base64.b64decode
    encode_into = base64.urlsafe_b64encode_into if urlsafe else base64.b64encode_into
    decode_into = base64.urlsafe_b64decode_into if urlsafe else base64.b64decode_into
    reference = stdlib_base64.urlsafe_b64encode if urlsafe else stdlib_base64.b64encode
    expected = reference(payload)

    assert encode(payload) == expected
    assert decode(expected) == payload
    if not urlsafe:
        assert base64.standard_b64encode(payload) == expected
        assert base64.standard_b64decode(expected.decode('ascii')) == payload

    for extra in (0, 1):
        encoded = bytearray(b'\xa5' * (len(expected) + extra))
        decoded = bytearray(b'\xa5' * (length + extra))
        assert encode_into(payload, encoded) == len(expected)
        assert encoded == expected + b'\xa5' * extra
        assert decode_into(expected, decoded) == length
        assert decoded == payload + b'\xa5' * extra


def test_alphabets_and_lenient_padding() -> None:
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


def _outcome(
    function: Callable[..., bytes], value: bytes | bytearray, altchars: bytes | None, validate: bool
) -> Observation:
    return observe_call(lambda: function(value, altchars, validate=validate))


def test_functions_keep_native_public_metadata() -> None:
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


def test_exports_have_stable_signatures() -> None:
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


def test_exports_match_typed_documentation() -> None:
    stub = Path(hashcodecs.__file__).with_name('_hashcodecs.pyi')
    declarations = ast.parse(stub.read_text(encoding='utf-8'), filename=str(stub))
    expected = {
        node.name: ast.get_docstring(node, clean=True)
        for node in declarations.body
        if isinstance(node, ast.FunctionDef) and 'b64' in node.name
    }

    assert set(expected) == set(base64.__all__)
    assert {name: getattr(base64, name).__doc__ for name in expected} == expected


def test_argument_errors_follow_public_signatures() -> None:
    with pytest.raises(TypeError, match=r"standard_b64encode\(\) missing required argument 's'"):
        dynamic_standard_b64encode()
    with pytest.raises(TypeError, match=r'urlsafe_b64encode\(\) takes at most 1 positional arguments'):
        dynamic_urlsafe_b64encode(b'', True)
    with pytest.raises(TypeError, match=r"standard_b64decode\(\) got an unexpected keyword argument 'unknown'"):
        dynamic_standard_b64decode(b'', unknown=True)
    with pytest.raises(TypeError, match=r"b64encode\(\) got multiple values for argument 's'"):
        dynamic_b64encode(b'', s=b'')
    with pytest.raises(TypeError, match=r"b64decode\(\) got multiple values for argument 's'"):
        dynamic_b64decode(b'YWJj', validate=True, s=b'ZGVm')
    with pytest.raises(TypeError, match=r"b64decode\(\) got multiple values for argument 'altchars'"):
        dynamic_b64decode(b'YWJj', b'-_', validate=True, altchars=b'@#')
    assert dynamic_b64decode(validate=True, s=b'YWJj') == b'abc'


def test_decode_into_distinguishes_omitted_options_from_none() -> None:
    parameters = inspect.signature(base64.b64decode_into).parameters
    assert parameters['validate'].default is not None
    assert parameters['ignorechars'].default is not None
    with pytest.raises(TypeError):
        dynamic_b64decode_into(b'YWJj', bytearray(3), ignorechars=None)

    omitted = bytearray(3)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'YWJj!', omitted, ignorechars=b'')
    explicit_none = bytearray(3)
    assert dynamic_b64decode_into(b'YWJj!', explicit_none, validate=None, ignorechars=b'') == 3
    assert explicit_none == b'abc'


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_python_315_b64decode_signature_matches_cpython() -> None:
    assert str(inspect.signature(base64.b64decode)) == str(inspect.signature(stdlib_base64.b64decode))


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
def test_decode_rejects_invalid_inputs(value: object, kwargs: dict[str, object], exception: type[Exception]) -> None:
    with pytest.raises(exception):
        dynamic_b64decode(value, **kwargs)


def test_encode_requires_contiguous_buffers() -> None:
    noncontiguous = memoryview(b'abcdef')[::2]
    with pytest.raises(BufferError):
        base64.b64encode(noncontiguous)
    with pytest.raises(TypeError if PYTHON_315 else BufferError):
        base64.b64encode(b'abc', memoryview(b'_-x_')[::2])
    with pytest.raises(ALTCHARS_ERROR):
        base64.b64encode(b'abc', b'_')


def test_encode_altchars_conversion_and_error_precedence_match_cpython() -> None:
    def outcome(function: Callable[..., bytes], value: object, altchars: object) -> bytes | type[Exception]:
        try:
            return function(value, altchars)  # type: ignore[arg-type]
        except Exception as error:
            return type(error)

    cases = (
        (b'abc', memoryview(b'-_').cast('H')),
        (b'abc', memoryview(b'----').cast('H')),
        (b'abc', memoryview(b'_-x_')[::2]),
        (b'abc', '-_'),
        (b'abc', object()),
        (object(), b'x'),
    )
    for value, altchars in cases:
        assert outcome(base64.b64encode, value, altchars) == outcome(stdlib_base64.b64encode, value, altchars)
