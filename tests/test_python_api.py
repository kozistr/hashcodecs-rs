import base64 as stdlib_base64
import binascii
from collections.abc import Callable
from typing import Any

import hashcodecs
import hashcodecs.base64 as base64
import hashcodecs.murmur3 as murmur3
import pytest


@pytest.mark.parametrize("payload", [b"", b"a", b"ab", b"abc", bytes(range(256))])
def test_base64_compatibility(payload: bytes) -> None:
    encoded = stdlib_base64.b64encode(payload)

    assert base64.b64encode(payload) == encoded
    assert base64.standard_b64encode(payload) == encoded
    assert base64.b64decode(encoded) == payload
    assert base64.b64decode(encoded, validate=True) == payload
    assert base64.standard_b64decode(encoded.decode("ascii")) == payload


def test_base64_variants_and_lenient_mode() -> None:
    assert base64.urlsafe_b64encode(b"\xfb\xff") == b"-_8="
    assert base64.urlsafe_b64decode(b"-_8=") == b"\xfb\xff"
    assert base64.b64decode(b"-_8=", b"-_", validate=True) == b"\xfb\xff"
    assert base64.b64encode(bytearray(b"abc")) == b"YWJj"
    assert base64.b64encode(memoryview(b"abc")) == b"YWJj"
    assert base64.b64decode(b"Y W\nJj", validate=False) == b"abc"
    assert base64.b64decode(b"YWJj====", validate=False) == b"abc"
    trailing_data = b"AA==anything after padding"
    assert _outcome(base64.b64decode, trailing_data, None, False) == _outcome(
        stdlib_base64.b64decode, trailing_data, None, False
    )
    assert base64.b64decode(b"AA=\n=") == b"\x00"
    assert base64.b64decode(b"++8=", b"-_", validate=True) == b"\xfb\xef"
    assert base64.b64decode(b"//8=", b"-_", validate=True) == b"\xff\xff"
    assert base64.b64decode(b"+-8=", b"-_", validate=True) == stdlib_base64.b64decode(b"+-8=", b"-_", validate=True)
    assert base64.b64decode(b"-_8=", "-_", validate=True) == b"\xfb\xff"
    assert base64.b64decode(b"++8=", b"++") == stdlib_base64.b64decode(b"++8=", b"++")
    assert base64.b64decode(b"++8=", b"++", validate=True) == stdlib_base64.b64decode(b"++8=", b"++", validate=True)
    assert base64.b64encode(b"\xfb\xff", b"@#") == b"@#8="


@pytest.mark.parametrize(
    ("value", "kwargs", "exception"),
    [
        (b"YWJj!", {"validate": True}, binascii.Error),
        (b"abc", {}, binascii.Error),
        ("\u2603", {}, ValueError),
        (b"abc", {"altchars": b"x"}, AssertionError),
        ([65, 66], {}, TypeError),
    ],
)
def test_base64_invalid_inputs(value: object, kwargs: dict[str, object], exception: type[Exception]) -> None:
    with pytest.raises(exception):
        base64.b64decode(value, **kwargs)  # type: ignore[arg-type]


def test_encode_requires_contiguous_buffers() -> None:
    noncontiguous = memoryview(b"abcdef")[::2]
    with pytest.raises(BufferError):
        base64.b64encode(noncontiguous)
    with pytest.raises(BufferError):
        base64.b64encode(b"abc", memoryview(b"_-x_")[::2])
    with pytest.raises(AssertionError):
        base64.b64encode(b"abc", b"_")


def test_murmur3_known_answers_and_buffer_inputs() -> None:
    assert hashcodecs.murmur3_32(b"hello") == 0x248BFA47
    assert murmur3.murmur3_32(b"hello") == 0x248BFA47
    assert hashcodecs.murmur3_32(memoryview(b"hello")) == 0x248BFA47
    assert hashcodecs.murmur3_x86_128_digest(bytes([1, 2, 3])) == bytes.fromhex("e16401f6334213b5334213b5334213b5")
    assert hashcodecs.murmur3_x64_128_digest(bytes([1, 2, 3])) == bytes.fromhex("a937130eef3e641a659a233c404a4e49")


def _outcome(function: Callable[..., bytes], value: bytes, altchars: bytes | None, validate: bool) -> Any:
    try:
        return function(value, altchars, validate=validate)
    except Exception as error:
        return type(error)


@pytest.mark.parametrize(
    "value",
    [
        b"",
        b"A",
        b"AA",
        b"AAA",
        b"AAAA",
        b"AA=",
        b"YQ=",
        b"YWI==",
        b"YWJj====",
        b"AAAA=AAA",
        b"AA==AA",
        b"=AAA",
        b"====",
        b"A===",
        b"AA===",
        b"AAA===",
        b"AAAA===",
        b"AA==junk",
        b"AA==!!",
        b"YW=Jj",
        b"YWJ=j",
        b"A=AAA",
        b"A==AAA",
        b"AA=A",
        b"AA==A",
        b"AAA=A",
        b"AAAA=A",
        b"AA=!!=",
        b"AA=! =",
        b"AA=Z=",
        b"AA=Z==",
        b"AA\n=",
        b"AA=\n=",
        b"++8=",
        b"--8=",
        b"//8=",
        b"__8=",
        b"+-8=",
        b"/_8=",
        b"Y W\nJj",
    ],
)
@pytest.mark.parametrize("altchars", [None, b"-_", b"@#", b"++", b"A_"])
@pytest.mark.parametrize("validate", [False, True])
def test_decode_edge_cases_match_cpython(value: bytes, altchars: bytes | None, validate: bool) -> None:
    expected = _outcome(stdlib_base64.b64decode, value, altchars, validate)
    actual = _outcome(base64.b64decode, value, altchars, validate)
    assert actual == expected


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
