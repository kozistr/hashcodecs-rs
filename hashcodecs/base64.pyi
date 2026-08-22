from _typeshed import ReadableBuffer

from ._hashcodecs import b64decode as b64decode
from ._hashcodecs import b64decode_batch as b64decode_batch
from ._hashcodecs import b64decode_batch_into as b64decode_batch_into
from ._hashcodecs import b64decode_into as b64decode_into
from ._hashcodecs import b64encode as b64encode
from ._hashcodecs import b64encode_batch as b64encode_batch
from ._hashcodecs import b64encode_batch_into as b64encode_batch_into
from ._hashcodecs import b64encode_into as b64encode_into
from ._hashcodecs import standard_b64decode as standard_b64decode
from ._hashcodecs import standard_b64decode_into as standard_b64decode_into
from ._hashcodecs import standard_b64encode as standard_b64encode
from ._hashcodecs import standard_b64encode_into as standard_b64encode_into
from ._hashcodecs import urlsafe_b64decode as urlsafe_b64decode
from ._hashcodecs import urlsafe_b64decode_into as urlsafe_b64decode_into
from ._hashcodecs import urlsafe_b64encode as urlsafe_b64encode
from ._hashcodecs import urlsafe_b64encode_into as urlsafe_b64encode_into

def standard_b64encode_batch(items: list[ReadableBuffer]) -> list[bytes]:
    """Encode a list of inputs with the padded standard Base64 alphabet.

    Results preserve input order. Processing stops at the first invalid item
    and no partial list is returned.

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
    ...

def standard_b64encode_batch_into(items: list[ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Encode standard Base64 into matching reusable bytearrays.

    The lists must have equal length and destinations must be distinct.
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
    ...

def standard_b64decode_batch(items: list[str | ReadableBuffer]) -> list[bytes]:
    """Decode a list of padded standard Base64 values.

    Non-alphabet characters are handled leniently. Results preserve input order
    and no partial list is returned when an item fails.

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
    ...

def standard_b64decode_batch_into(
    items: list[str | ReadableBuffer],
    outputs: list[bytearray],
) -> list[int]:
    """Decode standard Base64 into matching reusable bytearrays.

    The lists must have equal length and destinations must be distinct.
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
    ...

def urlsafe_b64encode_batch(items: list[ReadableBuffer]) -> list[bytes]:
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
    ...

def urlsafe_b64encode_batch_into(items: list[ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Encode URL-safe Base64 into matching reusable bytearrays.

    The lists must have equal length and destinations must be distinct.
    Processing is fail-fast and non-transactional.

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
    ...

def urlsafe_b64decode_batch(items: list[str | ReadableBuffer]) -> list[bytes]:
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
    ...

def urlsafe_b64decode_batch_into(
    items: list[str | ReadableBuffer],
    outputs: list[bytearray],
) -> list[int]:
    """Decode URL-safe Base64 into matching reusable bytearrays.

    The lists must have equal length and destinations must be distinct.
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
    ...
