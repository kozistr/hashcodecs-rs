"""A fast, API-compatible subset of Python's :mod:`base64` module."""

from ._hashcodecs import b64decode, b64encode


def standard_b64encode(s) -> bytes:
    """Encode *s* with the standard Base64 alphabet."""
    return b64encode(s)


def standard_b64decode(s) -> bytes:
    """Decode *s* with the standard Base64 alphabet."""
    return b64decode(s)


def urlsafe_b64encode(s) -> bytes:
    """Encode *s* with the URL-safe Base64 alphabet."""
    return b64encode(s, b"-_")


def urlsafe_b64decode(s) -> bytes:
    """Decode *s* with the URL-safe Base64 alphabet."""
    return b64decode(s, b"-_")


__all__ = [
    "b64decode",
    "b64encode",
    "standard_b64decode",
    "standard_b64encode",
    "urlsafe_b64decode",
    "urlsafe_b64encode",
]
