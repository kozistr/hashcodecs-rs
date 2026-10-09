import base64 as stdlib_base64
import binascii
import random
from collections.abc import Callable

import pytest
from base64_compat_harness import Observation, observe_call

import hashcodecs.base64 as base64


def test_unpadded_tails_and_custom_padding_symbol() -> None:
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


@pytest.mark.parametrize('altchars', [b'=_', b'_=', b'=='])
@pytest.mark.parametrize('encoded', [b'=', b'====', b'AA==', b'A===', b'YQ=='])
def test_lenient_decode_treats_custom_equals_as_alphabet(encoded: bytes, altchars: bytes) -> None:
    try:
        expected = stdlib_base64.b64decode(encoded, altchars)
    except binascii.Error:
        with pytest.raises(binascii.Error):
            base64.b64decode(encoded, altchars)
        with pytest.raises(binascii.Error):
            base64.b64decode_into(encoded, bytearray(16), altchars)
    else:
        assert base64.b64decode(encoded, altchars) == expected
        output = bytearray(len(expected))
        assert base64.b64decode_into(encoded, output, altchars) == len(expected)
        assert output == expected


def _outcome(
    function: Callable[..., bytes], value: bytes | bytearray, altchars: bytes | None, validate: bool
) -> Observation:
    return observe_call(lambda: function(value, altchars, validate=validate))


def _into_outcome(value: bytes | bytearray, altchars: bytes | None, validate: bool) -> Observation:
    output = bytearray(len(value))

    def decode_into() -> bytes:
        written = base64.b64decode_into(value, output, altchars, validate=validate)
        return bytes(output[:written])

    return observe_call(decode_into)


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
# Alphabet collisions have their own differential tests. Check padding errors
# through the standard decoder and a translated alphabet here.
@pytest.mark.parametrize('altchars', [None, b'@#'])
@pytest.mark.parametrize('validate', [False, True])
def test_decode_edge_cases_match_cpython(value: bytes, altchars: bytes | None, validate: bool) -> None:
    expected = _outcome(stdlib_base64.b64decode, value, altchars, validate)
    actual = _outcome(base64.b64decode, value, altchars, validate)
    assert actual == expected
    assert _into_outcome(value, altchars, validate) == expected
    mutable = bytearray(value)
    assert _outcome(base64.b64decode, mutable, altchars, validate) == expected
    assert _into_outcome(mutable, altchars, validate) == expected


@pytest.mark.parametrize('altchars', [None, b'-_', b'@#', b'=_'])
@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize('encoded', [b'A' * 4096 + b'AA!A', b'A' * 8192 + b'A'])
def test_staged_decode_errors_match_cpython(encoded: bytes, altchars: bytes | None, validate: bool) -> None:
    with pytest.raises(binascii.Error) as expected:
        stdlib_base64.b64decode(encoded, altchars, validate=validate)
    with pytest.raises(binascii.Error) as allocating:
        base64.b64decode(encoded, altchars, validate=validate)
    with pytest.raises(binascii.Error) as reusable:
        base64.b64decode_into(encoded, bytearray(len(encoded)), altchars, validate=validate)
    assert str(allocating.value) == str(expected.value)
    assert str(reusable.value) == str(expected.value)


def test_seeded_malformed_inputs_match_cpython() -> None:
    generator = random.Random(0xB64DEC0DE)
    alphabet = b'ABab09+/=_! \r\n-@#'
    altchars_cases = (None, b'+/', b'-_', b'@#', b'++', b'A_', b'=_', b'_=', b'==')

    for _ in range(256):
        value = bytes(generator.choice(alphabet) for _ in range(generator.randrange(33)))
        for altchars in altchars_cases:
            expected = _outcome(stdlib_base64.b64decode, value, altchars, False)
            assert _outcome(base64.b64decode, value, altchars, False) == expected
            assert _into_outcome(value, altchars, False) == expected
