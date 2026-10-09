import base64 as stdlib_base64
import binascii
from collections.abc import Callable

import pytest

import hashcodecs.base64 as base64

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into


@pytest.mark.parametrize('api', ['b64decode', 'b64decode_into', 'b64decode_batch', 'b64decode_batch_into'])
@pytest.mark.parametrize('text', ['é', '\ud800'])
def test_string_subclass_ascii_failures_are_normalized(api: str, text: str) -> None:
    class StringSubclass(str):
        pass

    value = StringSubclass(text)
    args = ([value],) if 'batch' in api else (value,)
    if api.endswith('_into'):
        args += ([bytearray(16)],) if 'batch' in api else (bytearray(16),)
    with pytest.raises(ValueError, match='ASCII') as error:
        getattr(base64, api)(*args)
    with pytest.raises(ValueError, match='ASCII') as reference:
        stdlib_base64.b64decode(value)
    assert type(error.value) is type(reference.value)
    assert str(error.value) == str(reference.value)


@pytest.mark.parametrize('api', ['b64decode', 'b64decode_into', 'b64decode_batch', 'b64decode_batch_into'])
@pytest.mark.parametrize('exception', [RuntimeError, ValueError, UnicodeDecodeError])
def test_string_subclass_preserves_unrelated_encode_exceptions(api: str, exception: type[Exception]) -> None:
    failure = (
        UnicodeDecodeError('ascii', b'\xff', 0, 1, 'custom decode failure')
        if exception is UnicodeDecodeError
        else exception('custom encode failure')
    )

    class RaisingString(str):
        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
            raise failure

    value = RaisingString('YWJj')
    args = ([value],) if 'batch' in api else (value,)
    if api.endswith('_into'):
        args += ([bytearray(16)],) if 'batch' in api else (bytearray(16),)
    with pytest.raises(exception) as error:
        getattr(base64, api)(*args)
    assert error.value is failure


@pytest.mark.parametrize(
    'length', [0, 1, 2, 3, 11, 12, 13, 23, 24, 25, 47, 48, 49, 63, 64, 65, 95, 96, 97, 255, 256, 257, 262145]
)
@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#'])
def test_ascii_string_decode_boundaries(length: int, validate: bool, altchars: bytes | None) -> None:
    payload = (bytes(range(256)) * (length // 256 + 1))[:length]
    encoded = stdlib_base64.b64encode(payload, altchars).decode('ascii')
    assert base64.b64decode(encoded, altchars, validate=validate) == payload
    for slack in (0, 7):
        output = bytearray(b'\xa5' * (length + slack))
        assert base64.b64decode_into(encoded, output, altchars, validate=validate) == length
        assert output == payload + b'\xa5' * slack
        if not validate and altchars is None:
            output[:] = b'\xa5' * len(output)
            assert base64.standard_b64decode(encoded) == payload
            assert base64.standard_b64decode_into(encoded, output) == length
            assert output == payload + b'\xa5' * slack

    if length:
        output = bytearray(b'\xa5' * (length - 1))
        with pytest.raises(ValueError, match='Base64 output requires'):
            base64.b64decode_into(encoded, output, altchars, validate=validate)
        assert output == b'\xa5' * (length - 1)


@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize(
    ('text', 'altchars'),
    # Text errors use both standard and custom decoding. Exercise alphabet
    # translation and invalid option lengths independently of those errors.
    [
        (text, altchars)
        for text in (
            'é',
            '\u0100',
            '\U0001f600',
            '\ud800',
            'A',
            'AA=',
            '=AAA',
            'AA==A',
            'AB==',
            'YW\x00Jj',
            'YWJj\r\n',
            'YW!Jj',
            '+/8=',
            '-_8=',
            '@#8=',
        )
        for altchars in (None, b'@#')
    ]
    + [
        ('+/8=', b'+/'),
        ('-_8=', b'-_'),
        ('+/8=', b'=='),
        ('+/8=', b'A_'),
        ('+/8=', b'++'),
        ('+/8=', b'/+'),
        ('+/8=', b'\x00\xff'),
    ]
    + [(text, altchars) for text in ('YWJj', 'é') for altchars in (b'', b'_', b'abc')],
)
def test_exact_string_decode_matches_cpython(text: str, validate: bool, altchars: bytes | None) -> None:
    failure = None
    try:
        expected = stdlib_base64.b64decode(text, altchars, validate=validate)
    except (ValueError, binascii.Error, AssertionError) as reference:
        failure = (type(reference), reference.args)
    if failure is not None:
        with pytest.raises(failure[0]) as actual:
            base64.b64decode(text, altchars, validate=validate)
        assert actual.value.args == failure[1]
        with pytest.raises(failure[0]) as actual:
            base64.b64decode_into(text, bytearray(16), altchars, validate=validate)
        assert actual.value.args == failure[1]
        return

    assert base64.b64decode(text, altchars, validate=validate) == expected
    output = bytearray(b'\xa5' * (len(expected) + 7))
    assert base64.b64decode_into(text, output, altchars, validate=validate) == len(expected)
    assert output == expected + b'\xa5' * 7


@pytest.mark.parametrize('text', ['YWJj', 'é', '\ud800'])
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#'])
def test_exact_string_normalization_precedes_argument_callbacks(text: str, altchars: bytes | None) -> None:
    events: list[str] = []
    output = bytearray(8)

    class Validation:
        def __bool__(self) -> bool:
            events.append('validate')
            output.clear()
            return True

    with pytest.raises(ValueError, match=r'Base64 output requires|ASCII') as actual:
        dynamic_b64decode_into(text, output, altchars, validate=Validation())
    if text == 'YWJj':
        assert events == ['validate']
        assert output == b''
        assert 'Base64 output requires' in str(actual.value)
    else:
        assert events == []
        assert output == bytearray(8)
        with pytest.raises(ValueError, match='ASCII') as reference:
            stdlib_base64.b64decode(text)
        assert actual.value.args == reference.value.args
