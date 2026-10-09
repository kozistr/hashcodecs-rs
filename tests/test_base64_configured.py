import base64 as stdlib_base64
import binascii
import re
import sys
import tracemalloc
from collections.abc import Callable, Mapping

import pytest
from base64_compat_harness import Observation, observe_call

import hashcodecs.base64 as base64

stdlib_b64decode: Callable[..., bytes] = stdlib_base64.b64decode

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into

PYTHON_315 = sys.version_info >= (3, 15)


@pytest.mark.parametrize('options', [{}, {'altchars': b'@#'}, {'ignorechars': b'!'}])
def test_large_discarded_prefix_does_not_reserve_an_input_sized_output(options: dict[str, object]) -> None:
    encoded = b'!' * (1024 * 1024) + b'YWJj'
    tracemalloc.start()
    try:
        assert dynamic_b64decode(encoded, **options) == b'abc'
        _, peak = tracemalloc.get_traced_memory()
    finally:
        tracemalloc.stop()
    assert peak < len(encoded) // 8


@pytest.mark.parametrize(
    ('altchars', 'remainder', 'kind'),
    [(altchars, remainder, bytes) for altchars in (None, b'-_', b'@#', b'=_', b'==') for remainder in range(3)]
    + [(altchars, 1, bytearray) for altchars in (None, b'@#', b'==')]
    + [(altchars, 2, memoryview) for altchars in (None, b'@#', b'==')],
)
def test_large_lenient_decode_preserves_exact_output_boundaries(
    altchars: bytes | None,
    remainder: int,
    kind: Callable[[bytes], bytes | bytearray | memoryview],
) -> None:
    payload = bytes(range(256)) * 1024 + b'x' * remainder
    encoded = stdlib_base64.b64encode(payload, altchars)
    for value in (encoded, encoded[:-4] + b'!!!!' + encoded[-4:], b'!!!!' + encoded):
        expected = stdlib_base64.b64decode(value, altchars)
        assert base64.b64decode(kind(value), altchars) == expected
        for extra in (0, 1):
            output = bytearray(b'.' * (len(expected) + extra))
            assert base64.b64decode_into(kind(value), output, altchars) == len(expected)
            assert output == expected + b'.' * extra
        output = bytearray(b'.' * (len(expected) - 1))
        with pytest.raises(ValueError, match='destination'):
            base64.b64decode_into(kind(value), output, altchars)
        assert output == b'.' * len(output)


def _decode_keyword_outcome(
    function: Callable[..., bytes],
    value: bytes,
    altchars: bytes | None,
    kwargs: Mapping[str, object],
) -> Observation:
    return observe_call(lambda: function(value, altchars, **kwargs))


def test_strict_custom_decode_preserves_detailed_errors_and_capacity_ordering() -> None:
    def error_outcome(function: Callable[[], object]) -> tuple[type[Exception], str]:
        try:
            function()
        except Exception as error:
            return type(error), str(error)
        raise AssertionError('expected decoding to fail')

    for encoded in (b'!!!!', b'AA=A', b'A', b'@@!A'):
        expected = error_outcome(lambda encoded=encoded: stdlib_base64.b64decode(encoded, b'@#', validate=True))
        assert error_outcome(lambda encoded=encoded: base64.b64decode(encoded, b'@#', validate=True)) == expected
        output = bytearray(len(encoded))
        assert (
            error_outcome(
                lambda encoded=encoded, output=output: base64.b64decode_into(encoded, output, b'@#', validate=True)
            )
            == expected
        )

    for encoded, padded, output_size, required in (
        (b'AA!A', True, 2, 3),
        (b'AA!', False, 1, 2),
        (b'====', True, 2, 3),
    ):
        output = bytearray([0xA5] * output_size)
        with pytest.raises(ValueError, match=rf'requires {required} bytes'):
            base64.b64decode_into(encoded, output, b'=_', validate=True, padded=padded)
        assert output == bytes([0xA5] * output_size)


@pytest.mark.parametrize('length', [63, 64, 65, 4095, 4096, 4097])
@pytest.mark.parametrize(
    ('altchars', 'kwargs'),
    [
        (None, {}),
        (b'-_', {}),
        (b'@#', {}),
        (b'@#', {'padded': False}),
        (None, {'canonical': True}),
        (None, {'ignorechars': b'! \r\n', 'validate': True}),
        (b'@#', {'ignorechars': b'! \r\n', 'validate': False}),
    ],
)
def test_decode_routes_preserve_exact_capacity_suffix_and_aliases(
    length: int, altchars: bytes | None, kwargs: dict[str, object]
) -> None:
    payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
    encoded = stdlib_base64.b64encode(payload, altchars)
    if kwargs.get('padded') is False:
        encoded = encoded.rstrip(b'=')
    if not kwargs.get('canonical'):
        encoded = b'!'.join(encoded[index : index + 76] for index in range(0, len(encoded), 76))
    assert dynamic_b64decode(encoded, altchars, **kwargs) == payload

    for extra in (0, 17):
        output = bytearray(b'\xa5' * (length + extra))
        assert dynamic_b64decode_into(encoded, output, altchars, **kwargs) == length
        assert output == payload + b'\xa5' * extra

    for as_view in (False, True):
        shared = bytearray(encoded)
        source = memoryview(shared) if as_view else shared
        assert dynamic_b64decode_into(source, shared, altchars, **kwargs) == length
        assert shared == payload + encoded[length:]


@pytest.mark.parametrize('prefix_length', [0, 12, 16, 28, 32, 60, 64, 76, 4096])
@pytest.mark.parametrize('altchars', [None, b'@#', b'=_', b'=='])
@pytest.mark.parametrize('tail', [b'YQ==AAAA', b'YQ=! =AAAA', b'YQ=AAAA', b'YQ', b'YQ==!'])
def test_lenient_sizing_preserves_padding_semantics_across_simd_runs(
    prefix_length: int, altchars: bytes | None, tail: bytes
) -> None:
    encoded = b'A' * prefix_length + b'!\r\n' + tail
    try:
        expected = stdlib_base64.b64decode(encoded, altchars)
    except binascii.Error as expected_error:
        with pytest.raises(binascii.Error, match=re.escape(str(expected_error))):
            base64.b64decode_into(encoded, bytearray(len(encoded)), altchars)
    else:
        for extra in (0, 7):
            output = bytearray(b'\xa5' * (len(expected) + extra))
            assert base64.b64decode_into(encoded, output, altchars) == len(expected)
            assert output == expected + b'\xa5' * extra
        if expected:
            output = bytearray(b'\xa5' * (len(expected) - 1))
            with pytest.raises(ValueError, match=rf'requires {len(expected)} bytes'):
                base64.b64decode_into(encoded, output, altchars)
            assert output == b'\xa5' * (len(expected) - 1)


@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize('encoded', [b'AB==', b'AAB=', b'A' * 4096 + b'AB=='])
def test_configured_canonical_failure_preserves_reusable_output(validate: bool, encoded: bytes) -> None:
    output = bytearray(b'\xa5' * len(encoded))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(encoded, output, validate=validate, canonical=True, ignorechars=b'! \r\n')
    assert output == b'\xa5' * len(encoded)


def test_strict_custom_decode_uses_staged_translation() -> None:
    payload = bytes((index * 37 + 11) & 0xFF for index in range(16_385))
    standard = stdlib_base64.b64encode(payload)
    custom = standard.translate(bytes.maketrans(b'+/', b'@#'))

    assert base64.b64decode(custom, b'@#', validate=True) == payload
    output = bytearray(len(payload))
    assert base64.b64decode_into(custom, output, b'@#', validate=True) == len(payload)
    assert output == payload

    unpadded = custom.rstrip(b'=')
    assert base64.b64decode(unpadded, b'@#', validate=True, padded=False) == payload
    output = bytearray(len(payload))
    assert base64.b64decode_into(unpadded, output, b'@#', validate=True, padded=False) == len(payload)
    assert output == payload

    valid_prefix = b'@' * 4096
    expected_prefix = stdlib_base64.b64decode(valid_prefix, b'@#', validate=True)
    malformed = valid_prefix + b'AA!A'
    output = bytearray([0xA5] * (len(malformed) // 4 * 3))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(malformed, output, b'@#', validate=True)
    assert output[: len(expected_prefix)] == expected_prefix


@pytest.mark.parametrize(
    'kwargs',
    [
        {'canonical': True},
        {'ignorechars': b''},
        {'validate': False, 'ignorechars': b''},
    ],
)
def test_standard_decode_into_strict_fast_paths(kwargs: dict[str, object]) -> None:
    payload = bytes(range(256)) * 64
    encoded = stdlib_base64.b64encode(payload)
    output = bytearray([0xA5] * (len(payload) + 16))

    assert dynamic_b64decode_into(encoded, output, **kwargs) == len(payload)
    assert output[: len(payload)] == payload
    assert output[len(payload) :] == bytes([0xA5] * 16)

    malformed = bytearray([0xA5] * 8)
    if kwargs.get('validate') is False:
        assert dynamic_b64decode_into(b'AB==!', malformed, **kwargs) == 1
        assert malformed == b'\x00' + bytes([0xA5] * 7)
    else:
        with pytest.raises(binascii.Error):
            dynamic_b64decode_into(b'AB==!', malformed, **kwargs)
        assert malformed == bytes([0xA5] * 8)


def test_configured_decode_native_staging_and_dispatch_paths() -> None:
    payload = bytes(range(256)) * 32 + b'native configured decoder tail'
    encoded = stdlib_base64.b64encode(payload)

    translated = encoded.translate(bytes.maketrans(b'+/', b'@#'))
    fast_input = b'!'.join(translated[index : index + 97] for index in range(0, len(translated), 97))
    assert base64.b64decode(fast_input, b'@#', ignorechars=b'!') == payload
    fast_output = bytearray(len(payload))
    assert base64.b64decode_into(fast_input, fast_output, b'@#', ignorechars=b'!') == len(payload)
    assert fast_output == payload

    ignored = b'!$%&'
    generic_input = ignored.join(encoded[index : index + 89] for index in range(0, len(encoded), 89))
    assert base64.b64decode(generic_input, ignorechars=ignored) == payload
    generic_output = bytearray(len(payload))
    assert base64.b64decode_into(generic_input, generic_output, ignorechars=ignored) == len(payload)
    assert generic_output == payload

    assert base64.b64decode(generic_input, validate=False, ignorechars=ignored) == payload
    lenient_output = bytearray(len(payload))
    assert base64.b64decode_into(generic_input, lenient_output, validate=False, ignorechars=ignored) == len(payload)
    assert lenient_output == payload

    alphanumeric = b'A' * 8192
    assert base64.b64decode(alphanumeric, ignorechars=ignored) == bytes(6144)

    mutable = bytearray(b'Y!WJj')
    assert base64.b64decode(mutable, ignorechars=b'!') == b'abc'


@pytest.mark.parametrize(
    'encoded',
    [
        b'AAAA?',
        b'A' * 4095 + b'?',
    ],
)
def test_configured_decode_native_rejects_invalid_staging(encoded: bytes) -> None:
    with pytest.raises(binascii.Error):
        base64.b64decode(encoded, ignorechars=b'!')
    output = bytearray([0xA5] * len(encoded))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(encoded, output, ignorechars=b'!')
    assert output == bytes([0xA5] * len(encoded))


@pytest.mark.parametrize('ignorechars', [b'=', b'=!', b'=!@', b'=!@#'])
@pytest.mark.parametrize(
    ('encoded', 'padded', 'expected'),
    [
        (b'=', True, b''),
        (b'==', True, b''),
        (b'AA=', False, b'\x00'),
        (b'AA==', True, b'\x00'),
        (b'AA===', True, b'\x00'),
        (b'AAA=', True, b'\x00\x00'),
        (b'AAA==', True, b'\x00\x00'),
        (b'AAAA=', True, bytes(3)),
        (b'=AA==', True, b'\x00'),
        (b'A=AAA', False, bytes(3)),
        (b'A=AAA', True, bytes(3)),
    ],
)
@pytest.mark.parametrize('validate', [None, True])
def test_strict_explicitly_ignored_equals(
    ignorechars: bytes, encoded: bytes, padded: bool, expected: bytes, validate: bool | None
) -> None:
    options = {'ignorechars': ignorechars, 'padded': padded}
    if validate is not None:
        options['validate'] = validate
    if PYTHON_315:
        assert stdlib_b64decode(encoded, **options) == expected
    assert dynamic_b64decode(encoded, **options) == expected
    output = bytearray(b'~' * (len(expected) + 8))
    assert dynamic_b64decode_into(encoded, output, **options) == len(expected)
    assert output == expected + b'~' * 8


@pytest.mark.parametrize('ignorechars', [b'', b'!', b'!?', b'!?~'])
def test_configured_decode_native_special_search_widths(ignorechars: bytes) -> None:
    encoded = b'Y' + ignorechars + b'WJj'
    assert base64.b64decode(encoded, b'@#', ignorechars=ignorechars) == b'abc'
    output = bytearray(3)
    assert base64.b64decode_into(encoded, output, b'@#', ignorechars=ignorechars) == 3
    assert output == b'abc'


def test_configured_decode_native_single_altchar_translation() -> None:
    assert base64.b64decode(b'@@8=', b'@/', ignorechars=b'') == b'\xfb\xef'
    output = bytearray(2)
    assert base64.b64decode_into(b'@@8=', output, b'@/', ignorechars=b'') == 2
    assert output == b'\xfb\xef'


def test_canonical_unpadded_decode_direct_paths() -> None:
    assert base64.b64decode(b'AAAA', padded=False, canonical=True) == b'\x00\x00\x00'
    output = bytearray(3)
    assert base64.b64decode_into(b'AAAA', output, padded=False, canonical=True) == 3
    assert output == b'\x00\x00\x00'

    for encoded in (b'A@', b'AA#'):
        with pytest.raises(binascii.Error):
            base64.b64decode(encoded, b'@#', padded=False, canonical=True)


def test_lenient_unpadded_decode_into_checks_final_output_size() -> None:
    with pytest.raises(ValueError, match='requires 1 bytes'):
        base64.b64decode_into(b'AA', bytearray(), validate=False, padded=False)


def test_configured_lenient_padding_matches_the_running_cpython() -> None:
    encoded = b'AA==AAAA'
    kwargs: dict[str, object] = {'validate': False, 'ignorechars': b'!$%&'}
    stdlib_kwargs = kwargs if PYTHON_315 else {'validate': False}
    expected = _decode_keyword_outcome(stdlib_base64.b64decode, encoded, None, stdlib_kwargs)
    assert _decode_keyword_outcome(base64.b64decode, encoded, None, kwargs) == expected

    output = bytearray(len(encoded))

    def decode_into() -> bytes:
        written = dynamic_b64decode_into(encoded, output, **kwargs)
        return bytes(output[:written])

    assert observe_call(decode_into) == expected
