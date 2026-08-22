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
    """Encode a list of inputs with the padded standard Base64 alphabet.

    Args:
        items: A list of contiguous bytes-like objects to encode.

    Returns:
        One newly allocated standard Base64 byte string per input item.

    Raises:
        TypeError: items is not a list or an item is not bytes-like.

    Examples:
        >>> standard_b64encode_batch([b'one', b'two'])
        [b'b25l', b'dHdv']
    """
    return b64encode_batch(items)


def standard_b64encode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Encode standard Base64 into matching reusable bytearrays.

    Processing is fail-fast and non-transactional, so earlier destinations
    remain modified when a later item fails.

    Args:
        items: A list of contiguous bytes-like objects to encode.
        outputs: An equal-length list of distinct destination bytearrays.

    Returns:
        The number of encoded bytes written to each destination.

    Raises:
        TypeError: A container, input item, or destination has an invalid type.
        ValueError: The list lengths differ or a destination is repeated or too
            small.

    Examples:
        >>> outputs = [bytearray(4), bytearray(4)]
        >>> standard_b64encode_batch_into([b'one', b'two'], outputs)
        [4, 4]
        >>> [bytes(output) for output in outputs]
        [b'b25l', b'dHdv']
    """
    return b64encode_batch_into(items, outputs)


def standard_b64decode_batch(items) -> list[bytes]:
    """Decode a list of padded standard Base64 values.

    Args:
        items: A list of ASCII strings or bytes-like Base64 values.

    Returns:
        One newly allocated decoded byte string per input item.

    Raises:
        binascii.Error: An item has invalid Base64 padding.
        TypeError: items is not a list or an item has an invalid type.
        ValueError: Text input contains non-ASCII characters.

    Examples:
        >>> standard_b64decode_batch([b'b25l', b'dHdv'])
        [b'one', b'two']
    """
    return b64decode_batch(items)


def standard_b64decode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Decode standard Base64 into matching reusable bytearrays.

    Processing is fail-fast and non-transactional; earlier destinations remain
    modified and the failing destination may be partly written.

    Args:
        items: A list of ASCII strings or bytes-like Base64 values.
        outputs: An equal-length list of distinct destination bytearrays.

    Returns:
        The number of decoded bytes written to each destination.

    Raises:
        binascii.Error: An item has invalid Base64 padding.
        TypeError: A container, input item, or destination has an invalid type.
        ValueError: The lists differ in length, a destination is repeated or too
            small, or text input is not ASCII.

    Examples:
        >>> outputs = [bytearray(3), bytearray(3)]
        >>> standard_b64decode_batch_into([b'b25l', b'dHdv'], outputs)
        [3, 3]
        >>> [bytes(output) for output in outputs]
        [b'one', b'two']
    """
    return b64decode_batch_into(items, outputs)


def urlsafe_b64encode_batch(items) -> list[bytes]:
    """Encode a list of inputs with the padded URL-safe Base64 alphabet.

    Args:
        items: A list of contiguous bytes-like objects to encode.

    Returns:
        One newly allocated URL-safe Base64 byte string per input item.

    Raises:
        TypeError: items is not a list or an item is not bytes-like.

    Examples:
        >>> urlsafe_b64encode_batch([bytes([251, 255]), b'two'])
        [b'-_8=', b'dHdv']
    """
    return b64encode_batch(items, b'-_')


def urlsafe_b64encode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Encode URL-safe Base64 into matching reusable bytearrays.

    Args:
        items: A list of contiguous bytes-like objects to encode.
        outputs: An equal-length list of distinct destination bytearrays.

    Returns:
        The number of encoded bytes written to each destination.

    Raises:
        TypeError: A container, input item, or destination has an invalid type.
        ValueError: The list lengths differ or a destination is repeated or too
            small.

    Examples:
        >>> output = bytearray(4)
        >>> urlsafe_b64encode_batch_into([bytes([251, 255])], [output])
        [4]
        >>> bytes(output)
        b'-_8='
    """
    return b64encode_batch_into(items, outputs, b'-_')


def urlsafe_b64decode_batch(items) -> list[bytes]:
    """Decode a list of padded URL-safe Base64 values.

    Args:
        items: A list of ASCII strings or bytes-like URL-safe Base64 values.

    Returns:
        One newly allocated decoded byte string per input item.

    Raises:
        binascii.Error: An item has invalid Base64 data or padding.
        TypeError: items is not a list or an item has an invalid type.
        ValueError: Text input contains non-ASCII characters.

    Examples:
        >>> urlsafe_b64decode_batch([b'-_8=', b'dHdv'])
        [b'\\xfb\\xff', b'two']
    """
    return b64decode_batch(items, b'-_')


def urlsafe_b64decode_batch_into(items, outputs: list[bytearray]) -> list[int]:
    """Decode URL-safe Base64 into matching reusable bytearrays.

    Processing is fail-fast and non-transactional; earlier destinations remain
    modified and the failing destination may be partly written.

    Args:
        items: A list of ASCII strings or bytes-like URL-safe Base64 values.
        outputs: An equal-length list of distinct destination bytearrays.

    Returns:
        The number of decoded bytes written to each destination.

    Raises:
        binascii.Error: An item has invalid Base64 data or padding.
        TypeError: A container, input item, or destination has an invalid type.
        ValueError: The lists differ in length, a destination is repeated or too
            small, or text input is not ASCII.

    Examples:
        >>> output = bytearray(2)
        >>> urlsafe_b64decode_batch_into([b'-_8='], [output])
        [2]
        >>> bytes(output)
        b'\\xfb\\xff'
    """
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
