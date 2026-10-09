import base64 as stdlib_base64
import binascii
import random
import re
import sys
from collections.abc import Callable
from functools import partial
from typing import Any

import pytest
from base64_compat_harness import (
    AltcharsHook,
    Invocation,
    Observation,
    SentinelError,
    observe,
)

import hashcodecs.base64 as base64

stdlib_b64encode: Callable[..., bytes] = stdlib_base64.b64encode

stdlib_b64decode: Callable[..., bytes] = stdlib_base64.b64decode

PYTHON_315 = sys.version_info >= (3, 15)

BASE64_ALPHABET = getattr(
    binascii, 'BASE64_ALPHABET', b'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/'
)


@pytest.mark.parametrize('validate', [False, True])
def test_decode_all_bytes(validate: bool) -> None:
    templates = (b'{}AAA', b'A{}AA', b'AA{}A', b'AAA{}', b'AA=={}')
    for template in templates:
        for value in range(256):
            encoded = template.replace(b'{}', bytes([value]))
            assert _decode_result(base64.b64decode, encoded, validate) == _decode_result(
                stdlib_base64.b64decode, encoded, validate
            )


def _decode_result(function: Callable[..., bytes], encoded: bytes, validate: bool) -> Observation:
    return observe(
        lambda: Invocation(
            lambda: function(encoded, validate=validate),
            [],
            SentinelError('sentinel'),
        )
    )


@pytest.mark.parametrize(
    'altchars', [b'@#', b'@@', b'==', b'=_', b'@=', b'AZ', b'/+', b'+@', b'++', b'//', b'\0\xff', b'\r\n']
)
@pytest.mark.parametrize('length', [15, 16, 17, 31, 32, 33, 63, 64, 65, 4095, 4096, 4097, 8192, 65536, 262145])
def test_custom_lenient_symbol_runs(altchars: bytes, length: int) -> None:
    symbols = b'AZaz09+/' + altchars
    run = (symbols * (length // len(symbols) + 1))[:length]
    for prefix in (b'', b'!A!', b'!AA!', b'!AAA!'):
        for tail in (b'', b'=', b'==', b'===', b'!AA==AAAA==', b'A=!=A==AAAA', b'!AAAA', b'!A'):
            encoded = prefix + run + tail
            try:
                expected = stdlib_b64decode(encoded, altchars)
            except binascii.Error as error:
                with pytest.raises(binascii.Error, match=f'^{re.escape(str(error))}$'):
                    base64.b64decode(encoded, altchars)
                continue

            assert base64.b64decode(encoded, altchars) == expected
            for extra in (0, 7):
                output = bytearray(b'\xa5' * (len(expected) + extra))
                assert base64.b64decode_into(encoded, output, altchars) == len(expected)
                assert output == expected + b'\xa5' * extra
            if expected:
                output = bytearray(b'\xa5' * (len(expected) - 1))
                with pytest.raises(ValueError, match='Base64 output requires'):
                    base64.b64decode_into(encoded, output, altchars)
                assert output == b'\xa5' * (len(expected) - 1)


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_altchar_byte_classes_match_cpython() -> None:
    inputs = (b'', b'AA==', b'+/8=', b'++8=', b'//8=', b'=w==')
    payloads = (b'', b'\x00', b'\xfb', b'\xff', b'\xfb\xff')
    # Every byte is tested in either slot. Cross only the classes where
    # translation can collide with padding, whitespace, or alphabet symbols.
    pairs = {(value, ord('#')) for value in range(256)}
    pairs.update((ord('@'), value) for value in range(256))
    special = b'Aa0+/=_@!\0\xff\r\n'
    pairs.update((first, second) for first in special for second in special)
    for first, second in sorted(pairs):
        altchars = bytes([first, second])
        for payload in payloads:
            assert base64.b64encode(payload, altchars) == stdlib_base64.b64encode(payload, altchars), altchars
        for encoded in inputs:
            expected = observe(
                lambda encoded=encoded, altchars=altchars: Invocation(
                    lambda: stdlib_base64.b64decode(encoded, altchars, validate=True),
                    [],
                    SentinelError('sentinel'),
                )
            )
            actual = observe(
                lambda encoded=encoded, altchars=altchars: Invocation(
                    lambda: base64.b64decode(encoded, altchars, validate=True),
                    [],
                    SentinelError('sentinel'),
                )
            )
            assert actual == expected, (altchars, encoded)


def _alphabet_decode_case(function: Callable[..., bytes], alphabet: bytes, encoded: bytes) -> Invocation:
    sentinel = SentinelError('sentinel')
    events: list[str] = []
    altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
    return Invocation(
        lambda: function(encoded, altchars, validate=True, ignorechars=b'!'),
        events,
        sentinel,
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    'alphabet',
    [
        BASE64_ALPHABET,
        BASE64_ALPHABET[::-1],
        b'Z' * 64,
        bytes(range(128, 192)),
        b'=' + BASE64_ALPHABET[1:],
        BASE64_ALPHABET[:-1] + b'=',
    ],
)
def test_constructed_alphabets(alphabet: bytes) -> None:
    def factory(function: Callable[..., bytes]) -> Invocation:
        sentinel = SentinelError('sentinel')
        events: list[str] = []
        altchars = AltcharsHook(b'-_', events, sentinel, alphabet=alphabet)
        return Invocation(lambda: function(bytes(range(64)), altchars), events, sentinel)

    assert observe(lambda: factory(base64.b64encode)) == observe(lambda: factory(stdlib_base64.b64encode))

    encoded_inputs = (
        b'',
        alphabet[:4],
        alphabet[:2] + b'==',
        alphabet[61:64] + b'=',
        b'====',
        alphabet[:3] + b'!',
    )
    for encoded in encoded_inputs:
        assert observe(partial(_alphabet_decode_case, base64.b64decode, alphabet, encoded)) == observe(
            partial(_alphabet_decode_case, stdlib_base64.b64decode, alphabet, encoded)
        )


def _configured_result(
    function: Callable[..., bytes],
    encoded: object,
    *args: object,
    **kwargs: object,
) -> Observation:
    return observe(
        lambda: Invocation(
            lambda: function(encoded, *args, **kwargs),
            [],
            SentinelError('sentinel'),
        )
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    'encoded',
    [
        b'',
        b'=',
        b'==',
        b'====',
        b'=AAA',
        b'A=AA',
        b'AA=A',
        b'AAA=',
        b'AA==',
        b'AA===',
        b'AA====',
        b'AA==A',
        b'AA==AAAA',
        b'AAAA=',
        b'AAAA==',
        b'AAAA===',
        b'YWJj=',
        b'YWJj==',
        b'Y=WJj',
        b'YW=Jj',
        b'====YWJj',
        b'YWJj====',
    ],
)
@pytest.mark.parametrize('as_text', [False, True])
def test_padding_positions(encoded: bytes, as_text: bool) -> None:
    source = encoded.decode('ascii') if as_text else encoded
    for validate in (False, True):
        for padded in (False, True):
            for canonical in (False, True):
                kwargs = {'validate': validate, 'padded': padded, 'canonical': canonical}
                assert _configured_result(base64.b64decode, source, **kwargs) == _configured_result(
                    stdlib_base64.b64decode, source, **kwargs
                )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize(
    ('encoded', 'altchars', 'ignorechars'),
    [
        (b'AYAA', None, b'A'),
        (b'YWJj', None, b'Y'),
        (b'+/8=', None, b'+'),
        (b'+/8=', None, b'/'),
        (b'AA==', None, b'='),
        (b'A=A=', None, b'A='),
        (b'-_8=', b'-_', b'-'),
        (b'-_8=', b'-_', b'_'),
        (b'-_8=', b'-_', b'-_='),
        (b'@@8=', b'@#', b'@'),
        (b'@#8=', b'@#', b'#='),
    ],
)
@pytest.mark.parametrize('as_text', [False, True])
def test_ignorechar_overlap(
    encoded: bytes,
    altchars: bytes | None,
    ignorechars: bytes,
    as_text: bool,
) -> None:
    source = encoded.decode('ascii') if as_text else encoded
    for validate in (False, True):
        kwargs = {'validate': validate, 'ignorechars': ignorechars}
        assert _configured_result(base64.b64decode, source, altchars, **kwargs) == _configured_result(
            stdlib_base64.b64decode, source, altchars, **kwargs
        )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_canonical_trailing_bits() -> None:
    for symbol in BASE64_ALPHABET:
        for encoded in (
            b'Q' + bytes([symbol]),
            b'Q' + bytes([symbol]) + b'==',
            b'QU' + bytes([symbol]),
            b'QU' + bytes([symbol]) + b'=',
        ):
            for validate in (False, True):
                for padded in (False, True):
                    for canonical in (False, True):
                        kwargs = {'validate': validate, 'padded': padded, 'canonical': canonical}
                        assert _configured_result(base64.b64decode, encoded, **kwargs) == _configured_result(
                            stdlib_base64.b64decode, encoded, **kwargs
                        )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
@pytest.mark.parametrize('encoded_length', [4092, 4095, 4096, 4097, 4100, 8191, 8192, 8193])
def test_staging_boundaries(encoded_length: int) -> None:
    symbols = (b'QUJD' * ((encoded_length + 3) // 4))[:encoded_length]
    insertion = min(encoded_length, 4096)
    encoded = symbols[:insertion] + b'!' + symbols[insertion:]
    kwargs = {'validate': True, 'ignorechars': b'!'}
    assert _configured_result(base64.b64decode, encoded, **kwargs) == _configured_result(
        stdlib_base64.b64decode, encoded, **kwargs
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_seeded_decode_mutations_match_cpython() -> None:
    randomizer = random.Random(0xB64C0DEC)
    altchar_choices = (None, b'-_', b'@#', b'++', b'=_', bytes([0x80, 0xFF]))
    ignorechar_choices = (None, b'', b'!', b' \n', b'=', bytes([0x80, 0xFF]))

    for _ in range(512):
        payload = randomizer.randbytes(randomizer.randrange(65))
        encoded = bytearray(stdlib_base64.b64encode(payload))
        if encoded and randomizer.randrange(2):
            del encoded[-randomizer.randrange(1, min(3, len(encoded)) + 1) :]
        for _ in range(randomizer.randrange(4)):
            encoded.insert(randomizer.randrange(len(encoded) + 1), randomizer.randrange(256))
        encoded.extend(b'=' * randomizer.randrange(4))

        altchars = randomizer.choice(altchar_choices)
        ignorechars = randomizer.choice(ignorechar_choices)
        kwargs: dict[str, object] = {
            'validate': bool(randomizer.randrange(2)),
            'padded': bool(randomizer.randrange(2)),
            'canonical': bool(randomizer.randrange(2)),
        }
        if ignorechars is not None:
            kwargs['ignorechars'] = ignorechars
        value = bytes(encoded)
        assert _configured_result(base64.b64decode, value, altchars, **kwargs) == _configured_result(
            stdlib_base64.b64decode, value, altchars, **kwargs
        )


def _encode_fuzz_case(
    function: Callable[..., Any],
    native_into: bool,
    payload: bytes,
    altchars: bytes | None,
    padded: bool,
    wrapcol: int,
) -> Invocation:
    sentinel = SentinelError('sentinel')
    output = bytearray([0xA5] * (((len(payload) + 2) // 3 * 4) * 2 + 5))
    initial = bytes(output)

    def call() -> int:
        if native_into:
            return function(payload, output, altchars, padded=padded, wrapcol=wrapcol)
        result = function(payload, altchars, padded=padded, wrapcol=wrapcol)
        output[: len(result)] = result
        return len(result)

    return Invocation(
        call,
        [],
        sentinel,
        mutables={'output': output},
        outputs={'output': output},
        output_initial={'output': initial},
    )


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_seeded_encode_options_match_cpython() -> None:
    randomizer = random.Random(0xE64C0DEC)
    altchar_choices = (None, b'-_', b'@#', b'++', b'==', b'A_', bytes([0x80, 0xFF]))
    wrapcol_choices = (-129, -1, 0, 1, 2, 3, 4, 15, 16, 17, 63, 64, 65, 127, 128, 129)

    for _ in range(512):
        payload = randomizer.randbytes(randomizer.randrange(257))
        altchars = randomizer.choice(altchar_choices)
        padded = bool(randomizer.randrange(2))
        wrapcol = randomizer.choice(wrapcol_choices)
        expected = observe(
            lambda payload=payload, altchars=altchars, padded=padded, wrapcol=wrapcol: Invocation(
                lambda: stdlib_b64encode(payload, altchars, padded=padded, wrapcol=wrapcol),
                [],
                SentinelError('sentinel'),
            )
        )
        actual = observe(
            lambda payload=payload, altchars=altchars, padded=padded, wrapcol=wrapcol: Invocation(
                lambda: base64.b64encode(payload, altchars, padded=padded, wrapcol=wrapcol),
                [],
                SentinelError('sentinel'),
            )
        )
        assert actual == expected
        assert observe(
            partial(_encode_fuzz_case, base64.b64encode_into, True, payload, altchars, padded, wrapcol)
        ) == observe(partial(_encode_fuzz_case, stdlib_base64.b64encode, False, payload, altchars, padded, wrapcol))


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 Base64 API')
def test_seeded_alphabets_match_cpython() -> None:
    randomizer = random.Random(0xA1F4BE7)

    for _ in range(128):
        alphabet = bytearray(randomizer.randbytes(64))
        alphabet[1] = alphabet[0]
        alphabet[2] = ord('=')
        alphabet[3] = randomizer.randrange(128, 256)
        payload = randomizer.randbytes(randomizer.randrange(97))

        expected_events: list[str] = []
        actual_events: list[str] = []
        expected_altchars = AltcharsHook(b'-_', expected_events, SentinelError('sentinel'), alphabet=bytes(alphabet))
        actual_altchars = AltcharsHook(b'-_', actual_events, SentinelError('sentinel'), alphabet=bytes(alphabet))
        expected = stdlib_base64.b64encode(payload, expected_altchars)
        assert base64.b64encode(payload, actual_altchars) == expected
        assert actual_events == expected_events

        assert observe(partial(_alphabet_decode_case, base64.b64decode, bytes(alphabet), expected)) == observe(
            partial(
                _alphabet_decode_case,
                stdlib_base64.b64decode,
                bytes(alphabet),
                expected,
            )
        )
