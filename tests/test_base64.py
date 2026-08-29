import ast
import base64 as stdlib_base64
import binascii
import builtins
import inspect
import random
import sys
import threading
import warnings
from array import array
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


def test_native_decode_into_handles_aliases_and_urlsafe_errors() -> None:
    assert base64.b64decode(bytearray(b'YWJj'), validate=True, padded=False) == b'abc'

    shared = bytearray(b'Y!WJj...')
    assert base64.b64decode_into(shared, shared) == 3
    assert shared == bytearray(b'abcJj...')

    shared = bytearray(b'YWJj')
    assert base64.b64decode_into(shared, shared, validate=True, padded=False) == 3
    assert shared == bytearray(b'abcj')

    shared = bytearray(b'AA=')
    with pytest.raises(binascii.Error):
        base64.b64decode_into(shared, shared, validate=True, padded=False)
    assert shared == bytearray(b'AA=')

    shared = bytearray(b'IJ!ZZ0')
    with pytest.raises(binascii.Error):
        base64.b64decode_into(shared, shared)

    output = bytearray([0xA5] * 2)
    assert base64.b64decode_into(b'AA', output, validate=False, padded=False) == 1
    assert output == bytearray(b'\x00\xa5')
    assert base64.b64decode_into(b'AA!', output, validate=False, padded=False) == 1
    assert output == bytearray(b'\x00\xa5')

    for encoded, padded in ((b'-_8=', True), (b'-_8', False)):
        output = bytearray([0xA5])
        with pytest.raises(ValueError, match='requires 2 bytes'):
            base64.b64decode_into(encoded, output, b'-_', validate=True, padded=padded)
        assert output == bytearray([0xA5])

    output = bytearray([0xA5] * 4)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'-_!', output, b'-_', validate=True, padded=False)
    assert output == bytearray([0xA5] * 4)


def test_buffer_conversion_uses_the_real_memoryview_type(monkeypatch: pytest.MonkeyPatch) -> None:
    encoded = memoryview(b'YWJj')
    payload = memoryview(b'abc')

    class FakeMemoryView:
        c_contiguous = True

        @staticmethod
        def tobytes() -> bytes:
            return b'abc'

    monkeypatch.setattr(builtins, 'memoryview', lambda value: FakeMemoryView())

    assert base64.b64encode(payload) == b'YWJj'
    assert base64.b64decode(encoded, validate=True) == b'abc'
    with pytest.raises(TypeError):
        base64.b64encode(object())


def test_exact_builtin_inputs_and_memoryviews_use_the_native_path() -> None:
    payload = b'abc'
    encoded = b'YWJj'
    for value in (payload, bytearray(payload), memoryview(payload)):
        assert base64.b64encode(value) == encoded
    for value in (encoded, bytearray(encoded), memoryview(encoded), encoded.decode('ascii')):
        assert base64.b64decode(value, validate=True) == payload

    # A memoryview can overlap a reusable destination. The native path must
    # snapshot it before writing, just as the previous copied path did.
    shared = bytearray(b'YWJj....')
    assert base64.b64decode_into(memoryview(shared)[:4], shared, validate=True) == 3
    assert shared[:3] == b'abc'

    shared = bytearray(b'YWJj')
    assert base64.b64decode_into(memoryview(shared), shared, validate=True) == 3
    assert shared == b'abcj'
    assert base64.b64decode(memoryview(b'xYWJj')[1:], validate=True) == b'abc'
    assert base64.b64encode(memoryview(b'abcd').cast('I', shape=[])) == b'YWJjZA=='
    assert base64.b64decode(memoryview(b'YWJj').cast('I', shape=[]), validate=True) == b'abc'
    assert base64.b64encode(array('B', b'abc')) == b'YWJj'
    assert base64.b64decode(array('B', b'YWJj'), validate=True) == b'abc'

    large_payload = bytes(range(256)) * 256
    large_encoded = stdlib_base64.b64encode(large_payload)
    assert base64.b64encode(memoryview(large_payload)) == large_encoded
    assert base64.b64decode(memoryview(large_encoded), validate=True) == large_payload


def _assert_mutable_input_race_is_serialized(
    operation: Callable[[], bytes],
    value: bytearray,
    states: tuple[bytes, bytes],
    expected: set[bytes],
) -> None:
    start = threading.Barrier(2)
    failures: list[BaseException | bytes] = []

    def run_operation() -> None:
        try:
            start.wait()
            for _ in range(32):
                result = operation()
                if result not in expected:
                    failures.append(result[:64])
                    return
        except BaseException as error:
            failures.append(error)

    def resize_input() -> None:
        start.wait()
        for index in range(32):
            value[:] = states[index % 2]

    worker = threading.Thread(target=run_operation)
    mutator = threading.Thread(target=resize_input)
    worker.start()
    mutator.start()
    worker.join(timeout=30)
    mutator.join(timeout=30)

    assert not worker.is_alive()
    assert not mutator.is_alive()
    assert not failures


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
def test_base64_bytearray_resize_races_are_serialized() -> None:
    raw_states = (b'a' * (1024 * 1024), b'b' * (1024 * 1024 + 3))
    raw = bytearray(raw_states[0])
    encoded_states = tuple(stdlib_base64.b64encode(state) for state in raw_states)
    _assert_mutable_input_race_is_serialized(
        lambda: base64.b64encode(raw),
        raw,
        raw_states,
        set(encoded_states),
    )

    encoded = bytearray(encoded_states[0])
    _assert_mutable_input_race_is_serialized(
        lambda: base64.b64decode(encoded, validate=True),
        encoded,
        encoded_states,
        set(raw_states),
    )

    raw = bytearray(raw_states[0])
    encode_output = bytearray(len(encoded_states[1]))
    _assert_mutable_input_race_is_serialized(
        lambda: bytes(encode_output[: base64.b64encode_into(raw, encode_output)]),
        raw,
        raw_states,
        set(encoded_states),
    )

    encoded = bytearray(encoded_states[0])
    decode_output = bytearray(len(raw_states[1]))
    _assert_mutable_input_race_is_serialized(
        lambda: bytes(decode_output[: base64.b64decode_into(encoded, decode_output, validate=True)]),
        encoded,
        encoded_states,
        set(raw_states),
    )


def test_subclasses_and_python_buffer_hooks_follow_cpython_slow_path() -> None:
    class BytesSubclass(bytes):
        pass

    class ByteArraySubclass(bytearray):
        pass

    class StringSubclass(str):
        def __new__(cls, value: str):
            instance = super().__new__(cls, value)
            instance.encode_calls = 0
            return instance

        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
            self.encode_calls += 1
            return super().encode(encoding, errors)

    assert base64.b64encode(BytesSubclass(b'abc')) == b'YWJj'
    assert base64.b64encode(ByteArraySubclass(b'abc')) == b'YWJj'
    text = StringSubclass('YWJj')
    assert base64.b64decode(text, validate=True) == b'abc'
    assert text.encode_calls == 1

    class RaisingString(str):
        def encode(self, encoding: str = 'utf-8', errors: str = 'strict') -> bytes:
            raise RuntimeError('custom encode failure')

    with pytest.raises(RuntimeError, match='custom encode failure'):
        base64.b64decode(RaisingString('YWJj'))

    if sys.version_info >= (3, 12):  # noqa: UP036 - package supports Python 3.10.

        class BufferHook:
            def __init__(self, value: bytes) -> None:
                self.value = value
                self.calls = 0

            def __buffer__(self, flags: int) -> memoryview:
                self.calls += 1
                return memoryview(self.value)

        encoded = BufferHook(b'YWJj')
        payload = BufferHook(b'abc')
        assert base64.b64decode(encoded, validate=True) == b'abc'
        assert base64.b64encode(payload) == b'YWJj'
        assert encoded.calls == 1
        assert payload.calls == 1

        class ExportFailure(RuntimeError):
            pass

        class RaisingBuffer:
            def __init__(self) -> None:
                self.calls = 0

            def __buffer__(self, flags: int) -> memoryview:
                self.calls += 1
                raise ExportFailure('custom export failure')

        raising = RaisingBuffer()
        with pytest.raises(ExportFailure, match='custom export failure'):
            base64.b64encode(raising)
        assert raising.calls == 1

        class BufferList(list):
            def __buffer__(self, flags: int) -> memoryview:
                return memoryview(b'abc')

        assert base64.b64encode(BufferList()) == b'YWJj'


def test_large_ascii_string_decode() -> None:
    payload = bytes(range(256)) * 512
    encoded = stdlib_base64.b64encode(payload).decode('ascii')
    assert base64.b64decode(encoded, validate=True) == payload
    with pytest.raises(ValueError, match='only ASCII'):
        base64.b64decode('\ud800')


def test_base64_into_variants_and_errors() -> None:
    encoded = bytearray([0xA5] * 12)
    assert base64.b64encode_into(b'abc', encoded) == 4
    assert encoded[:4] == b'YWJj'
    assert encoded[4:] == bytearray([0xA5] * 8)
    assert base64.b64encode_into(bytearray(b'abc'), encoded) == 4
    assert base64.standard_b64encode_into(b'abc', encoded) == 4
    assert hashcodecs.b64encode_into(b'\xfb\xff', encoded, b'@#') == 4
    assert encoded[:4] == b'@#8='
    assert hashcodecs.b64encode_into(b'\xfb\xff', encoded, b'/+') == 4
    assert encoded[:4] == b'/+8='
    assert base64.b64encode_into(b'\xfb\xff', encoded, b'+/') == 4
    assert encoded[:4] == b'+/8='
    assert base64.urlsafe_b64encode_into(b'\xfb\xff', encoded) == 4
    assert encoded[:4] == b'-_8='

    decoded = bytearray([0xA5] * 8)
    assert base64.b64decode_into(b'Y W\nJj', decoded) == 3
    assert decoded[:3] == b'abc'
    assert decoded[3:] == bytearray([0xA5] * 5)
    assert base64.standard_b64decode_into(b'YWJj', decoded) == 3
    assert hashcodecs.b64decode_into(b'@#8=', decoded, b'@#', validate=True) == 2
    assert decoded[:2] == b'\xfb\xff'
    assert base64.b64decode_into(b'+/8=', decoded, b'+/', validate=True) == 2
    assert decoded[:2] == b'\xfb\xff'
    assert base64.urlsafe_b64decode_into(b'-_8=', decoded) == 2
    assert decoded[:2] == b'\xfb\xff'
    assert base64.b64decode_into(b'YWJj', decoded, b'-_', padded=False) == 3
    assert decoded[:3] == b'abc'

    with pytest.raises(ValueError, match='requires 4 bytes'):
        base64.b64encode_into(b'abc', bytearray(3))
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'YWJj', bytearray(2), validate=True)
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'YWJj', bytearray(2), validate=True, padded=False)
    undersized = bytearray(b'XX')
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'Y!WJj', undersized)
    assert undersized == b'XX'
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'YWJj!', bytearray(8), validate=True)
    with pytest.raises(TypeError):
        base64.b64encode_into(b'abc', b'....')  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        base64.b64decode_into(b'YWJj', memoryview(bytearray(3)))  # type: ignore[arg-type]


def test_base64_into_handles_aliases_and_every_short_length() -> None:
    empty = bytearray()
    assert base64.b64encode(empty) == b''
    assert base64.b64decode(empty) == b''
    assert base64.b64encode_into(empty, empty) == 0
    assert base64.b64decode_into(empty, empty) == 0

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


def test_lenient_decode_into_uses_final_size_and_preserves_suffix() -> None:
    # The strict SIMD probe sees 128 structurally aligned bytes, while the
    # lenient decoder discards four invalid bytes and produces only 93 bytes.
    encoded = b'A' * 80 + b'!!!!' + b'A' * 44
    expected = stdlib_base64.b64decode(encoded)
    assert len(expected) == 93

    exact = bytearray(len(expected))
    assert base64.b64decode_into(encoded, exact) == len(expected)
    assert exact == expected

    canary = 0xA5
    guarded = bytearray([canary] * (len(expected) + 16))
    assert base64.b64decode_into(encoded, guarded) == len(expected)
    assert guarded[: len(expected)] == expected
    assert guarded[len(expected) :] == bytes([canary] * 16)


@pytest.mark.skipif(not PYTHON_315, reason='requires the new lenient sizing path')
@pytest.mark.parametrize(
    'encoded',
    [
        b'Y!WJj',
        b'A' * 12 + b'!!!!',
        b'A' * 28 + b'!!!!',
    ],
)
def test_lenient_decode_into_covers_counter_widths(encoded: bytes) -> None:
    expected = stdlib_base64.b64decode(encoded, validate=False)
    output = bytearray(len(expected))

    written = base64.b64decode_into(encoded, output, validate=False)

    assert written == len(expected)
    assert output == expected


def test_lenient_decode_into_exact_eight_symbol_boundary() -> None:
    with pytest.raises(ValueError, match='requires 6 bytes'):
        base64.b64decode_into(b'AAAAAAAA', bytearray(5), validate=False)


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


def test_advanced_decode_fallback_edge_cases() -> None:
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


def test_advanced_decode_native_staging_and_dispatch_paths() -> None:
    payload = bytes(range(256)) * 32 + b'native advanced decoder tail'
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
def test_advanced_decode_native_rejects_invalid_staging(encoded: bytes) -> None:
    with pytest.raises(binascii.Error):
        base64.b64decode(encoded, ignorechars=b'!')
    output = bytearray([0xA5] * len(encoded))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(encoded, output, ignorechars=b'!')
    assert output == bytes([0xA5] * len(encoded))


@pytest.mark.parametrize('ignorechars', [b'', b'!', b'!?', b'!?~'])
def test_advanced_decode_native_special_search_widths(ignorechars: bytes) -> None:
    encoded = b'Y' + ignorechars + b'WJj'
    assert base64.b64decode(encoded, b'@#', ignorechars=ignorechars) == b'abc'
    output = bytearray(3)
    assert base64.b64decode_into(encoded, output, b'@#', ignorechars=ignorechars) == 3
    assert output == b'abc'


def test_advanced_decode_native_single_altchar_translation() -> None:
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


def test_advanced_lenient_padding_matches_the_running_cpython() -> None:
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


def test_advanced_decode_bypasses_binascii(monkeypatch: pytest.MonkeyPatch) -> None:
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
