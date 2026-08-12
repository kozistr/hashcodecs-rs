"""A fast, API-compatible subset of Python's :mod:`base64` module."""

from ._hashcodecs import (
    b64decode,
    b64decode_batch,
    b64decode_batch_into,
    b64decode_into,
    b64encode,
    b64encode_batch,
    b64encode_batch_into,
    b64encode_into,
    standard_b64decode,
    standard_b64decode_into,
    standard_b64encode,
    standard_b64encode_into,
    urlsafe_b64decode,
    urlsafe_b64decode_into,
    urlsafe_b64encode,
    urlsafe_b64encode_into,
)

for _function in (
    standard_b64decode,
    standard_b64decode_into,
    standard_b64encode,
    standard_b64encode_into,
    urlsafe_b64decode,
    urlsafe_b64decode_into,
    urlsafe_b64encode,
    urlsafe_b64encode_into,
):
    _function.__module__ = __name__
del _function


def standard_b64encode_batch(items) -> list[bytes]:
    """Encode a list of inputs with the standard Base64 alphabet."""
    return b64encode_batch(items)


def standard_b64encode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Encode a list of inputs with the standard alphabet into reusable outputs."""
    return b64encode_batch_into(items, outputs)


def standard_b64decode_batch(items) -> list[bytes]:
    """Decode a list of inputs with the standard Base64 alphabet."""
    return b64decode_batch(items)


def standard_b64decode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Decode a list of standard Base64 inputs into reusable outputs."""
    return b64decode_batch_into(items, outputs)


def urlsafe_b64encode_batch(items) -> list[bytes]:
    """Encode a list of inputs with the URL-safe Base64 alphabet."""
    return b64encode_batch(items, b'-_')


def urlsafe_b64encode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Encode a list of inputs with the URL-safe alphabet into reusable outputs."""
    return b64encode_batch_into(items, outputs, b'-_')


def urlsafe_b64decode_batch(items) -> list[bytes]:
    """Decode a list of inputs with the URL-safe Base64 alphabet."""
    return b64decode_batch(items, b'-_')


def urlsafe_b64decode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Decode a list of URL-safe Base64 inputs into reusable outputs."""
    return b64decode_batch_into(items, outputs, b'-_')


__all__ = [
    'b64decode',
    'b64decode_batch',
    'b64decode_batch_into',
    'b64decode_into',
    'b64encode',
    'b64encode_batch',
    'b64encode_batch_into',
    'b64encode_into',
    'standard_b64decode',
    'standard_b64decode_batch',
    'standard_b64decode_batch_into',
    'standard_b64decode_into',
    'standard_b64encode',
    'standard_b64encode_batch',
    'standard_b64encode_batch_into',
    'standard_b64encode_into',
    'urlsafe_b64decode',
    'urlsafe_b64decode_batch',
    'urlsafe_b64decode_batch_into',
    'urlsafe_b64decode_into',
    'urlsafe_b64encode',
    'urlsafe_b64encode_batch',
    'urlsafe_b64encode_batch_into',
    'urlsafe_b64encode_into',
]
