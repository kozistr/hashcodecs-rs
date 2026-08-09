"""A fast, API-compatible subset of Python's :mod:`base64` module."""

import sys

from ._hashcodecs import b64decode, b64decode_into, b64encode, b64encode_into

_URLSAFE_PADDED_DEFAULT = sys.version_info < (3, 15)


def standard_b64encode(s) -> bytes:
    """Encode *s* with the standard Base64 alphabet."""
    return b64encode(s)


def standard_b64encode_into(s, output: bytearray) -> int:
    """Encode *s* with the standard Base64 alphabet into *output*."""
    return b64encode_into(s, output)


def standard_b64decode(s) -> bytes:
    """Decode *s* with the standard Base64 alphabet."""
    return b64decode(s)


def standard_b64decode_into(s, output: bytearray) -> int:
    """Decode standard Base64 *s* into *output*."""
    return b64decode_into(s, output)


def urlsafe_b64encode(s, *, padded: bool = True) -> bytes:
    """Encode *s* with the URL-safe Base64 alphabet."""
    return b64encode(s, b'-_', padded=padded)


def urlsafe_b64encode_into(s, output: bytearray, *, padded: bool = True) -> int:
    """Encode *s* with the URL-safe Base64 alphabet into *output*."""
    return b64encode_into(s, output, b'-_', padded=padded)


def urlsafe_b64decode(s, *, padded: bool = _URLSAFE_PADDED_DEFAULT) -> bytes:
    """Decode *s* with the URL-safe Base64 alphabet."""
    return b64decode(s, b'-_', padded=padded)


def urlsafe_b64decode_into(s, output: bytearray, *, padded: bool = _URLSAFE_PADDED_DEFAULT) -> int:
    """Decode URL-safe Base64 *s* into *output*."""
    return b64decode_into(s, output, b'-_', padded=padded)


__all__ = [
    'b64decode',
    'b64decode_into',
    'b64encode',
    'b64encode_into',
    'standard_b64decode',
    'standard_b64decode_into',
    'standard_b64encode',
    'standard_b64encode_into',
    'urlsafe_b64decode',
    'urlsafe_b64decode_into',
    'urlsafe_b64encode',
    'urlsafe_b64encode_into',
]
