import base64 as stdlib_base64
import binascii
import random
import re
import sys
import warnings
from collections.abc import Callable
from typing import Any

import pytest

import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)
FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()
ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError
BASE64_DETACH_THRESHOLD = 256 * 1024

GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


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


def test_common_lenient_decoding_does_not_call_binascii(monkeypatch: pytest.MonkeyPatch) -> None:
    def fail_binascii(*args: object, **kwargs: object) -> bytes:
        raise AssertionError(f'unexpected binascii decode: {args!r} {kwargs!r}')

    monkeypatch.setattr(binascii, 'a2b_base64', fail_binascii)

    noisy = b'Y!W \nJj'
    assert base64.b64decode(noisy) == b'abc'
    assert base64.standard_b64decode(noisy) == b'abc'
    assert base64.b64decode(b'@\n#8=', b'@#') == b'\xfb\xff'
    assert base64.urlsafe_b64decode(b'-\n_8=') == b'\xfb\xff'

    output = bytearray(b'.' * 8)
    assert base64.b64decode_into(noisy, output) == 3
    assert output == b'abc.....'

    assert base64.b64decode_batch([noisy, b'Z GVm']) == [b'abc', b'def']
    outputs = [bytearray(b'....'), bytearray(b'....')]
    assert base64.b64decode_batch_into([noisy, b'Z GVm'], outputs) == [3, 3]
    assert outputs == [b'abc.', b'def.']


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
@pytest.mark.parametrize('altchars', [None, b'+/', b'-_', b'@#', b'++', b'A_'])
@pytest.mark.parametrize('validate', [False, True])
def test_decode_edge_cases_match_cpython(value: bytes, altchars: bytes | None, validate: bool) -> None:
    expected = _outcome(stdlib_base64.b64decode, value, altchars, validate)
    actual = _outcome(base64.b64decode, value, altchars, validate)
    assert actual == expected
    assert _into_outcome(value, altchars, validate) == expected
    mutable = bytearray(value)
    assert _outcome(base64.b64decode, mutable, altchars, validate) == expected
    assert _into_outcome(mutable, altchars, validate) == expected


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


@pytest.mark.parametrize('altchars', [None, b'-_', b'@#', b'=_'])
@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize('encoded', [b'A', b'AA=!', b'AA!A', b'AAAAA', b'A' * 4096 + b'AA!A'])
def test_decode_fallback_preserves_cpython_error_messages(
    encoded: bytes, altchars: bytes | None, validate: bool
) -> None:
    with pytest.raises(binascii.Error) as expected:
        stdlib_base64.b64decode(encoded, altchars, validate=validate)
    with pytest.raises(binascii.Error) as allocating:
        base64.b64decode(encoded, altchars, validate=validate)
    with pytest.raises(binascii.Error) as reusable:
        base64.b64decode_into(encoded, bytearray(len(encoded)), altchars, validate=validate)
    assert str(allocating.value) == str(expected.value)
    assert str(reusable.value) == str(expected.value)


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
    assert base64.b64decode(encoded, altchars, **kwargs) == payload

    for extra in (0, 17):
        output = bytearray(b'\xa5' * (length + extra))
        assert base64.b64decode_into(encoded, output, altchars, **kwargs) == length
        assert output == payload + b'\xa5' * extra

    for as_view in (False, True):
        shared = bytearray(encoded)
        source = memoryview(shared) if as_view else shared
        assert base64.b64decode_into(source, shared, altchars, **kwargs) == length
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


def test_generated_lenient_inputs_match_cpython() -> None:
    generator = random.Random(0xB64DEC0DE)
    alphabet = b'ABab09+/=_! \r\n-@#'
    altchars_cases = (None, b'+/', b'-_', b'@#', b'++', b'A_', b'=_', b'_=', b'==')

    for _ in range(2_000):
        value = bytes(generator.choice(alphabet) for _ in range(generator.randrange(33)))
        for altchars in altchars_cases:
            expected = _outcome(stdlib_base64.b64decode, value, altchars, False)
            assert _outcome(base64.b64decode, value, altchars, False) == expected
            assert _into_outcome(value, altchars, False) == expected


def test_all_short_payload_lengths_match_cpython() -> None:
    for length in range(1025):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        standard = stdlib_base64.b64encode(payload)
        urlsafe = stdlib_base64.urlsafe_b64encode(payload)
        assert base64.b64encode(payload) == standard
        assert base64.b64decode(standard) == payload
        assert base64.urlsafe_b64encode(payload) == urlsafe
        assert base64.urlsafe_b64decode(urlsafe) == payload


@pytest.mark.skipif(FREE_THREADED, reason='requires a GIL-enabled CPython build')
def test_large_base64_calls_release_the_gil(assert_releases_gil: GILProgressAssertion) -> None:
    payload = bytes(range(256)) * (BASE64_DETACH_THRESHOLD // 256)
    encoded = stdlib_base64.b64encode(payload)

    assert_releases_gil(lambda: base64.b64encode(payload), encoded, 128)
    assert_releases_gil(lambda: base64.b64decode(encoded, validate=True), payload, 128)
    custom = encoded.translate(bytes.maketrans(b'+/', b'@#'))
    assert_releases_gil(lambda: base64.b64decode(custom, b'@#', validate=True), payload, 128)


def test_configured_decode_fallback_edge_cases() -> None:
    assert base64.b64decode(b'@!#8', b'@#', padded=False, ignorechars=b'!') == b'\xfb\xff'
    output = bytearray([0xA5] * 4)
    assert base64.b64decode_into(b'@!#8', output, b'@#', padded=False, ignorechars=b'!') == 2
    assert output == bytearray(b'\xfb\xff\xa5\xa5')

    assert base64.b64decode(b'AA=', padded=False, validate=False, ignorechars=b'!') == b'\x00'
    with pytest.raises(binascii.Error):
        base64.b64decode(b'A!', padded=False, validate=False, ignorechars=b'!')

    assert base64.b64decode(b'', canonical=True) == b''
    assert base64.b64decode(b'AAA', padded=False, canonical=True) == b'\x00\x00'
    assert base64.b64decode(b'AAAA', canonical=True) == b'\x00\x00\x00'

    assert base64.b64decode(b'YWJj', b'@#', validate=True, padded=False) == b'abc'
    output = bytearray(3)
    assert base64.b64decode_into(b'YWJj', output, b'@#', validate=True, padded=False) == 3
    assert output == b'abc'


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

    assert base64.b64decode_into(encoded, output, **kwargs) == len(payload)
    assert output[: len(payload)] == payload
    assert output[len(payload) :] == bytes([0xA5] * 16)

    malformed = bytearray([0xA5] * 8)
    if kwargs.get('validate') is False:
        assert base64.b64decode_into(b'AB==!', malformed, **kwargs) == 1
        assert malformed == b'\x00' + bytes([0xA5] * 7)
    else:
        with pytest.raises(binascii.Error):
            base64.b64decode_into(b'AB==!', malformed, **kwargs)
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
    try:
        written = base64.b64decode_into(encoded, output, **kwargs)
        actual: bytes | type[Exception] = bytes(output[:written])
    except Exception as error:
        actual = type(error)
    assert actual == expected


def test_decode_fallback_lazily_recovers_exact_memoryview_owner(monkeypatch: pytest.MonkeyPatch) -> None:
    observed: list[object] = []

    def record_input(data: object, *args: object, **kwargs: object) -> bytes:
        observed.append(data)
        return b''

    monkeypatch.setattr(binascii, 'a2b_base64', record_input)

    encoded = b'abc'
    assert base64.b64decode(memoryview(encoded)) == b''
    assert observed[-1] is encoded

    mutable = bytearray(encoded)
    assert base64.b64decode(memoryview(mutable)) == b''
    assert observed[-1] == encoded
    assert isinstance(observed[-1], bytes)

    sliced_owner = b'xabc'
    assert base64.b64decode(memoryview(sliced_owner)[1:]) == b''
    assert observed[-1] == encoded
    assert observed[-1] is not sliced_owner

    output = bytearray(1)
    assert base64.b64decode_into(memoryview(encoded), output) == 0
    assert output == b'\x00'


def test_configured_decode_bypasses_binascii(monkeypatch: pytest.MonkeyPatch) -> None:
    def unexpected_fallback(*args: object, **kwargs: object) -> bytes:
        raise AssertionError((args, kwargs))

    monkeypatch.setattr(binascii, 'a2b_base64', unexpected_fallback)
    encoded = b'Y!WJj'
    assert base64.b64decode(encoded, ignorechars=b'!') == b'abc'

    output = bytearray(3)
    assert base64.b64decode_into(encoded, output, ignorechars=b'!') == 3
    assert output == b'abc'

    undersized = bytearray([0xA5] * 2)
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(encoded, undersized, ignorechars=b'!')
    assert undersized == bytearray([0xA5] * 2)

    shared = bytearray(encoded)
    assert base64.b64decode_into(shared, shared, ignorechars=b'!') == 3
    assert shared[:3] == b'abc'

    view = memoryview(encoded)
    assert base64.b64decode(view, ignorechars=b'!') == b'abc'

    with pytest.raises(binascii.Error):
        base64.b64decode(b'A!', padded=False, validate=False, ignorechars=b'!')
    unchanged = bytearray([0xA5] * 4)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'A!', unchanged, padded=False, validate=False, ignorechars=b'!')
    assert unchanged == bytearray([0xA5] * 4)
