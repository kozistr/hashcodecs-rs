import binascii
import sys
from collections.abc import Callable

import pytest

import hashcodecs.base64 as base64

dynamic_b64decode: Callable[..., bytes] = base64.b64decode

dynamic_b64decode_into: Callable[..., int] = base64.b64decode_into

PYTHON_315 = sys.version_info >= (3, 15)


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


@pytest.mark.skipif(not PYTHON_315, reason='requires the CPython 3.15 binascii API')
def test_configured_fallback_accepts_bytes(monkeypatch: pytest.MonkeyPatch) -> None:
    calls: list[tuple[object, dict[str, object]]] = []

    def fallback(data: object, **kwargs: object) -> bytes:
        calls.append((data, kwargs))
        return b'abc'

    monkeypatch.setattr(binascii, 'a2b_base64', fallback)
    encoded = b'AA==AAAA'
    options = {'validate': False, 'ignorechars': b'!'}

    assert dynamic_b64decode(encoded, **options) == b'abc'

    output = bytearray(b'.....')
    assert dynamic_b64decode_into(encoded, output, **options) == 3
    assert output == b'abc..'
    expected = {
        'strict_mode': False,
        'padded': True,
        'canonical': False,
        'ignorechars': b'!',
    }
    assert calls == [(encoded, expected)] * 2


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


def test_configured_decode_bypasses_binascii_on_success_and_capacity_errors(monkeypatch: pytest.MonkeyPatch) -> None:
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

    monkeypatch.undo()
    with pytest.raises(binascii.Error):
        base64.b64decode(b'A!', padded=False, validate=False, ignorechars=b'!')
    unchanged = bytearray([0xA5] * 4)
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'A!', unchanged, padded=False, validate=False, ignorechars=b'!')
    assert unchanged == bytearray([0xA5] * 4)
