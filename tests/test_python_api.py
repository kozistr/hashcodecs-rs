import base64 as stdlib_base64
import binascii
from collections.abc import Callable
from typing import Any

import hashcodecs
import hashcodecs.base64 as base64
import hashcodecs.murmur3 as murmur3
import pytest


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
    assert base64.b64encode(bytearray(b'abc')) == b'YWJj'
    assert base64.b64encode(memoryview(b'abc')) == b'YWJj'
    assert base64.b64decode(b'Y W\nJj', validate=False) == b'abc'
    assert base64.b64decode(b'YWJj====', validate=False) == b'abc'
    trailing_data = b'AA==anything after padding'
    assert _outcome(base64.b64decode, trailing_data, None, False) == _outcome(
        stdlib_base64.b64decode, trailing_data, None, False
    )
    assert base64.b64decode(b'AA=\n=') == b'\x00'
    assert base64.b64decode(b'++8=', b'-_', validate=True) == b'\xfb\xef'
    assert base64.b64decode(b'//8=', b'-_', validate=True) == b'\xff\xff'
    assert base64.b64decode(b'+-8=', b'-_', validate=True) == stdlib_base64.b64decode(b'+-8=', b'-_', validate=True)
    assert base64.b64decode(b'-_8=', '-_', validate=True) == b'\xfb\xff'
    assert base64.b64decode(b'++8=', b'++') == stdlib_base64.b64decode(b'++8=', b'++')
    assert base64.b64decode(b'++8=', b'++', validate=True) == stdlib_base64.b64decode(b'++8=', b'++', validate=True)
    assert base64.b64encode(b'\xfb\xff', b'@#') == b'@#8='


def test_base64_into_variants_and_errors() -> None:
    encoded = bytearray([0xA5] * 12)
    assert base64.b64encode_into(b'abc', encoded) == 4
    assert encoded[:4] == b'YWJj'
    assert encoded[4:] == bytearray([0xA5] * 8)
    assert base64.standard_b64encode_into(b'abc', encoded) == 4
    assert hashcodecs.b64encode_into(b'\xfb\xff', encoded, b'@#') == 4
    assert encoded[:4] == b'@#8='
    assert base64.urlsafe_b64encode_into(b'\xfb\xff', encoded) == 4
    assert encoded[:4] == b'-_8='

    decoded = bytearray([0xA5] * 8)
    assert base64.b64decode_into(b'Y W\nJj', decoded) == 3
    assert decoded[:3] == b'abc'
    assert decoded[3:] == bytearray([0xA5] * 5)
    assert base64.standard_b64decode_into(b'YWJj', decoded) == 3
    assert hashcodecs.b64decode_into(b'@#8=', decoded, b'@#', validate=True) == 2
    assert decoded[:2] == b'\xfb\xff'
    assert base64.urlsafe_b64decode_into(b'-_8=', decoded) == 2
    assert decoded[:2] == b'\xfb\xff'

    with pytest.raises(ValueError, match='requires 4 bytes'):
        base64.b64encode_into(b'abc', bytearray(3))
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'YWJj', bytearray(2), validate=True)
    with pytest.raises(ValueError, match='requires 3 bytes'):
        base64.b64decode_into(b'Y W\nJj', bytearray(2))
    with pytest.raises(binascii.Error):
        base64.b64decode_into(b'YWJj!', bytearray(8), validate=True)
    with pytest.raises(TypeError):
        base64.b64encode_into(b'abc', b'....')  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        base64.b64decode_into(b'YWJj', memoryview(bytearray(3)))  # type: ignore[arg-type]


def test_base64_into_handles_aliases_and_every_short_length() -> None:
    shared = bytearray(8)
    shared[:3] = b'abc'
    assert base64.b64encode_into(memoryview(shared)[:3], shared) == 4
    assert shared[:4] == b'YWJj'
    assert base64.b64decode_into(memoryview(shared)[:4], shared, validate=True) == 3
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


@pytest.mark.parametrize(
    ('value', 'kwargs', 'exception'),
    [
        (b'YWJj!', {'validate': True}, binascii.Error),
        (b'abc', {}, binascii.Error),
        ('\u2603', {}, ValueError),
        (b'abc', {'altchars': b'x'}, AssertionError),
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
    with pytest.raises(BufferError):
        base64.b64encode(b'abc', memoryview(b'_-x_')[::2])
    with pytest.raises(AssertionError):
        base64.b64encode(b'abc', b'_')


def test_murmur3_known_answers_and_buffer_inputs() -> None:
    assert hashcodecs.murmur3_32(b'hello') == 0x248BFA47
    assert murmur3.murmur3_32(b'hello') == 0x248BFA47
    assert hashcodecs.murmur3_32(memoryview(b'hello')) == 0x248BFA47
    assert hashcodecs.murmur3_x86_128_digest(bytes([1, 2, 3])) == bytes.fromhex('e16401f6334213b5334213b5334213b5')
    assert hashcodecs.murmur3_x64_128_digest(bytes([1, 2, 3])) == bytes.fromhex('a937130eef3e641a659a233c404a4e49')


@pytest.mark.parametrize(
    ('constructor', 'one_shot', 'name', 'digest_size', 'block_size'),
    [
        (
            murmur3.murmur3_x86_32,
            lambda data, seed=0: murmur3.murmur3_32(data, seed).to_bytes(4, 'little'),
            'murmur3_x86_32',
            4,
            4,
        ),
        (murmur3.murmur3_x86_128, murmur3.murmur3_x86_128_digest, 'murmur3_x86_128', 16, 16),
        (murmur3.murmur3_x64_128, murmur3.murmur3_x64_128_digest, 'murmur3_x64_128', 16, 16),
    ],
)
def test_hashlib_style_murmur3_api(
    constructor: Callable[..., Any],
    one_shot: Callable[..., bytes],
    name: str,
    digest_size: int,
    block_size: int,
) -> None:
    hasher = constructor(memoryview(b'prefix'), 42)
    assert hasher.update(bytearray(b'-suffix')) is None
    expected = one_shot(b'prefix-suffix', 42)
    assert hasher.digest() == expected
    assert hasher.hexdigest() == expected.hex()
    assert hasher.digest() == expected
    assert hasher.name == name
    assert hasher.digest_size == digest_size
    assert hasher.block_size == block_size

    snapshot = constructor(b'prefix', seed=42).copy()
    snapshot.update(b'-suffix')
    assert snapshot.digest() == expected
    assert constructor().digest() == one_shot(b'')


def test_incremental_murmur3_matches_one_shot_at_every_boundary() -> None:
    constructors = (
        (
            hashcodecs.murmur3_x86_32,
            lambda data, seed: hashcodecs.murmur3_32(data, seed).to_bytes(4, 'little'),
        ),
        (hashcodecs.murmur3_x86_128, hashcodecs.murmur3_x86_128_digest),
        (hashcodecs.murmur3_x64_128, hashcodecs.murmur3_x64_128_digest),
    )
    for length in range(65):
        payload = bytes((index * 43 + 5) & 0xFF for index in range(length))
        for constructor, one_shot in constructors:
            actual = constructor(seed=0xFEEDBEEF)
            for offset in range(0, length, 3):
                actual.update(payload[offset : offset + 3])
            assert actual.digest() == one_shot(payload, 0xFEEDBEEF)


@pytest.mark.parametrize('constructor', [murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128])
def test_hashlib_style_murmur3_rejects_invalid_inputs(constructor: Callable[..., Any]) -> None:
    with pytest.raises(TypeError):
        constructor([1, 2, 3])
    with pytest.raises(OverflowError):
        constructor(seed=-1)
    with pytest.raises(TypeError):
        constructor().update([1, 2, 3])


def _outcome(function: Callable[..., bytes], value: bytes, altchars: bytes | None, validate: bool) -> Any:
    try:
        return function(value, altchars, validate=validate)
    except Exception as error:
        return type(error)


def _into_outcome(value: bytes, altchars: bytes | None, validate: bool) -> Any:
    output = bytearray(len(value))
    try:
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
@pytest.mark.parametrize('altchars', [None, b'-_', b'@#', b'++', b'A_'])
@pytest.mark.parametrize('validate', [False, True])
def test_decode_edge_cases_match_cpython(value: bytes, altchars: bytes | None, validate: bool) -> None:
    expected = _outcome(stdlib_base64.b64decode, value, altchars, validate)
    actual = _outcome(base64.b64decode, value, altchars, validate)
    assert actual == expected
    assert _into_outcome(value, altchars, validate) == expected


def test_all_short_payload_lengths_match_cpython() -> None:
    for length in range(1025):
        payload = bytes((index * 37 + 11) & 0xFF for index in range(length))
        standard = stdlib_base64.b64encode(payload)
        urlsafe = stdlib_base64.urlsafe_b64encode(payload)
        assert base64.b64encode(payload) == standard
        assert base64.b64decode(standard) == payload
        assert base64.urlsafe_b64encode(payload) == urlsafe
        assert base64.urlsafe_b64decode(urlsafe) == payload


def test_large_calls_cross_the_gil_release_threshold() -> None:
    payload = bytes(range(256)) * 257
    encoded = stdlib_base64.b64encode(payload)
    assert base64.b64encode(payload) == encoded
    assert base64.b64decode(encoded) == payload
    assert hashcodecs.murmur3_32(payload, 42) == hashcodecs.murmur3_32(bytearray(payload), 42)
    assert hashcodecs.murmur3_x86_128_digest(payload, 42) == hashcodecs.murmur3_x86_128_digest(bytearray(payload), 42)
    assert hashcodecs.murmur3_x64_128_digest(payload, 42) == hashcodecs.murmur3_x64_128_digest(bytearray(payload), 42)
    for constructor in (murmur3.murmur3_x86_32, murmur3.murmur3_x86_128, murmur3.murmur3_x64_128):
        hasher = constructor(seed=42)
        hasher.update(payload)
        assert hasher.digest() == constructor(payload, 42).digest()
