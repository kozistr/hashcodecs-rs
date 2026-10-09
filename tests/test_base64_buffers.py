import base64 as stdlib_base64
import binascii
import builtins
import sys
from array import array
from collections.abc import Callable

import pytest

import hashcodecs
import hashcodecs.base64 as base64

PYTHON_315 = sys.version_info >= (3, 15)

dynamic_b64encode: Callable[..., bytes] = base64.b64encode

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


@pytest.mark.parametrize('length', [1024, 65536, 262144, 1048576])
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#'])
def test_sliced_decode_preserves_exact_output_boundaries(length: int, altchars: bytes | None) -> None:
    payload = (bytes(range(256)) * (length // 256 + 1))[:length]
    encoded = stdlib_base64.b64encode(payload, altchars)
    view = memoryview(b'!' + encoded + b'!')[1:-1]
    assert base64.b64decode(view, altchars, validate=True) == payload
    output = bytearray(b'\xa5' * (length + 1))
    assert base64.b64decode_into(view, output, altchars, validate=True) == length
    assert output == payload + b'\xa5'
    with pytest.raises(ValueError, match='destination'):
        base64.b64decode_into(view, bytearray(length - 1), altchars, validate=True)


@pytest.mark.parametrize('valid_symbols', [32, 128, 4096])
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#', b'=='])
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


def test_into_wrappers_and_argument_errors() -> None:
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


def test_into_snapshots_empty_and_overlapping_inputs() -> None:
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


@pytest.mark.parametrize('length', [0, 1, 2, 12, 24, 48, 64, 96, 256, 1024, 262145])
@pytest.mark.parametrize('validate', [False, True])
@pytest.mark.parametrize('padded', [False, True])
@pytest.mark.parametrize('view', [False, True])
def test_decode_into_snapshots_aliases(length: int, validate: bool, padded: bool, view: bool) -> None:
    expected = (bytes(range(256)) * (length // 256 + 1))[:length]
    encoded = stdlib_base64.b64encode(expected)
    if not padded:
        encoded = encoded.rstrip(b'=')
    if not validate:
        encoded = b'!'.join(encoded[offset : offset + 16] for offset in range(0, len(encoded), 16))
    output = bytearray(encoded)
    source = memoryview(output).toreadonly() if view else output

    assert base64.b64decode_into(source, output, validate=validate, padded=padded) == length
    assert output == expected + encoded[length:]


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
