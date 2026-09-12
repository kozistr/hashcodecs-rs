import base64 as stdlib_base64
import binascii
import builtins
import sys
import threading
from array import array
from collections.abc import Callable, Sequence
from typing import cast

import pytest

import hashcodecs
import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)
FREE_THREADED = not getattr(sys, '_is_gil_enabled', lambda: True)()
ALTCHARS_ERROR = ValueError if PYTHON_315 else AssertionError
BASE64_DETACH_THRESHOLD = 256 * 1024
GILProgressAssertion = Callable[[Callable[[], object], object, int], None]
dynamic_b64encode: Callable[..., bytes] = base64.b64encode
dynamic_b64decode: Callable[..., bytes] = base64.b64decode
dynamic_b64encode_into: Callable[..., int] = base64.b64encode_into
dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into


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
        dynamic_b64encode(object())


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
    states: Sequence[bytes],
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


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('urlsafe', [False, True])
def test_free_threaded_encode_snapshot_respects_legacy_callback_order(urlsafe: bool) -> None:
    source = bytearray(b'abc')

    class Padded:
        def __bool__(self) -> bool:
            source[:] = b'def'
            return True

    function = cast(Callable[..., bytes], base64.urlsafe_b64encode if urlsafe else base64.b64encode)
    expected = b'ZGVm' if PYTHON_315 or urlsafe else b'YWJj'
    assert function(source, padded=Padded()) == expected


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
@pytest.mark.parametrize('reusable', [False, True])
def test_free_threaded_decode_snapshots_ignorechars_after_canonical_callback(reusable: bool) -> None:
    ignorechars = bytearray(b'!')

    class Canonical:
        def __bool__(self) -> bool:
            ignorechars[:] = b'?'
            return False

    output = bytearray(3)
    if reusable:
        assert dynamic_b64decode_into(b'Y?WJj', output, ignorechars=ignorechars, canonical=Canonical()) == 3
        assert output == b'abc'
    else:
        assert dynamic_b64decode(b'Y?WJj', ignorechars=ignorechars, canonical=Canonical()) == b'abc'


@pytest.mark.skipif(not FREE_THREADED, reason='requires a free-threaded CPython build')
def test_free_threaded_standard_into_snapshot() -> None:
    source = bytearray(b'YWJj')
    output = bytearray(b'.....')

    assert base64.standard_b64decode_into(source, output) == 3
    assert source == b'YWJj'
    assert output == b'abc..'


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python buffer release hooks')
@pytest.mark.parametrize(
    ('operation', 'value', 'expected'),
    [
        (base64.standard_b64encode_into, b'abc', b'YWJj'),
        (base64.b64encode_into, b'abc', b'YWJj'),
        (base64.urlsafe_b64encode_into, b'abc', b'YWJj'),
        (base64.standard_b64decode_into, b'YWJj', b'abc'),
        (base64.b64decode_into, b'YWJj', b'abc'),
        (base64.urlsafe_b64decode_into, b'YWJj', b'abc'),
    ],
)
@pytest.mark.parametrize('remaining_capacity', [0, 3, 4, 16])
def test_reentrant_buffer_release_hooks_run_before_reusable_output_writes(
    operation: Callable[..., int], value: bytes, expected: bytes, remaining_capacity: int
) -> None:
    output = bytearray(16)
    releases = []

    class Buffer:
        def __buffer__(self, flags: int) -> memoryview:
            return memoryview(value)

        def __release_buffer__(self, view: memoryview) -> None:
            releases.append(True)
            output[:] = b'.' * remaining_capacity

    if remaining_capacity < len(expected):
        with pytest.raises(
            ValueError, match=f'requires {len(expected)} bytes but the destination has {remaining_capacity}'
        ):
            operation(Buffer(), output)
        assert output == b'.' * remaining_capacity
    else:
        assert operation(Buffer(), output) == len(expected)
        assert output == expected + b'.' * (remaining_capacity - len(expected))
    assert releases == [True]


@pytest.mark.skipif(sys.version_info < (3, 12), reason='requires Python buffer release hooks')
def test_reentrant_ignorechars_release_hook_runs_before_reusable_output_write() -> None:
    class Buffer:
        def __init__(self, value: bytes, output: bytearray) -> None:
            self.value = value
            self.output = output

        def __buffer__(self, flags: int) -> memoryview:
            return memoryview(self.value)

        def __release_buffer__(self, view: memoryview) -> None:
            self.output.clear()

    decoded = bytearray(3)
    with pytest.raises(ValueError, match='requires 3 bytes but the destination has 0'):
        base64.b64decode_into(b'YWJj', decoded, ignorechars=Buffer(b'', decoded))
    assert decoded == b''


def test_subclasses_and_python_buffer_hooks_follow_cpython_slow_path() -> None:
    class BytesSubclass(bytes):
        pass

    class ByteArraySubclass(bytearray):
        pass

    class StringSubclass(str):
        encode_calls: int

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


@pytest.mark.parametrize('valid_symbols', [32, 128])
@pytest.mark.parametrize('altchars', [None, b'-_'])
def test_lenient_retry_preserves_suffix_after_avx2_validation_boundary(
    valid_symbols: int, altchars: bytes | None
) -> None:
    encoded = b'A' * valid_symbols + b'!' * 32
    expected = bytes(valid_symbols // 4 * 3)
    output = bytearray(b'~' * (len(expected) + 40))
    written = base64.b64decode_into(encoded, output, altchars)
    assert written == len(expected)
    assert output == expected + b'~' * 40

    outputs = [bytearray(b'~' * (len(expected) + 40)) for _ in range(2)]
    assert base64.b64decode_batch_into([encoded] * 2, outputs, altchars) == [len(expected)] * 2
    assert outputs == [expected + b'~' * 40] * 2


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
        dynamic_b64encode_into(b'abc', b'....')
    with pytest.raises(TypeError):
        dynamic_b64decode_into(b'YWJj', memoryview(bytearray(3)))


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
