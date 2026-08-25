from _typeshed import ReadableBuffer

def b64encode(
    s: ReadableBuffer,
    altchars: ReadableBuffer | None = None,
    *,
    padded: bool = True,
    wrapcol: int = 0,
) -> bytes:
    """Encode a bytes-like object as Base64.

    The standard RFC 4648 alphabet is used unless altchars replaces its "+"
    and "/" characters. Padding and fixed-width line wrapping can be
    controlled for protocols that require a particular wire format.

    Args:
        s: Contiguous bytes-like data to encode.
        altchars: A two-byte replacement for "+" and "/", or None for the
            standard alphabet.
        padded: Append trailing "=" padding when required.
        wrapcol: Maximum encoded characters per line. Zero disables wrapping.

    Returns:
        Newly allocated Base64-encoded bytes.

    Raises:
        TypeError: An argument has an unsupported type.
        ValueError: altchars is not exactly two bytes or wrapcol is negative.

    Examples:
        >>> b64encode(b'hello')
        b'aGVsbG8='
        >>> b64encode(b'hello', padded=False, wrapcol=4)
        b'aGVs\\nbG8'
    """
    ...

def b64encode_batch(items: list[ReadableBuffer], altchars: ReadableBuffer | None = None) -> list[bytes]:
    """Encode a list of bytes-like objects as padded Base64.

    Every item uses the same alphabet and results preserve input order. The
    operation stops at the first invalid item and discards partial results.

    Args:
        items: A list of contiguous bytes-like objects to encode.
        altchars: A two-byte replacement for "+" and "/", or None for the
            standard alphabet.

    Returns:
        One newly allocated Base64 byte string per input item.

    Raises:
        TypeError: items is not a list or an item is not bytes-like.
        ValueError: altchars is not exactly two bytes.

    Examples:
        >>> b64encode_batch([b'one', b'two'])
        [b'b25l', b'dHdv']
    """
    ...

def b64encode_batch_into(
    items: list[ReadableBuffer],
    outputs: list[bytearray],
    altchars: ReadableBuffer | None = None,
) -> list[int]:
    """Encode each item into a matching reusable bytearray.

    The two lists must have equal length and every destination must be a
    distinct bytearray. Processing is fail-fast and non-transactional, so
    earlier destinations remain modified when a later item fails.

    Args:
        items: A list of contiguous bytes-like objects to encode.
        outputs: An equal-length list of distinct destination bytearrays.
        altchars: A two-byte replacement for "+" and "/", or None for the
            standard alphabet.

    Returns:
        The number of bytes written to each destination, in input order.

    Raises:
        TypeError: A container, input item, or destination has an invalid type.
        ValueError: The list lengths differ, a destination is repeated or too
            small, or altchars is not exactly two bytes.

    Examples:
        >>> outputs = [bytearray(4), bytearray(4)]
        >>> b64encode_batch_into([b'one', b'two'], outputs)
        [4, 4]
        >>> [bytes(output) for output in outputs]
        [b'b25l', b'dHdv']
    """
    ...

def b64encode_into(
    s: ReadableBuffer,
    output: bytearray,
    altchars: ReadableBuffer | None = None,
    *,
    padded: bool = True,
    wrapcol: int = 0,
) -> int:
    """Encode a bytes-like object as Base64 into a reusable bytearray.

    The destination keeps its size. Only the returned prefix is overwritten;
    bytes after that prefix remain unchanged.

    Args:
        s: Contiguous bytes-like data to encode.
        output: Destination bytearray with room for the complete result.
        altchars: A two-byte replacement for "+" and "/", or None for the
            standard alphabet.
        padded: Append trailing "=" padding when required.
        wrapcol: Maximum encoded characters per line. Zero disables wrapping.

    Returns:
        The number of bytes written to output.

    Raises:
        TypeError: An argument has an unsupported type.
        ValueError: The destination is too small, altchars is not exactly two
            bytes, or wrapcol is negative.

    Examples:
        >>> output = bytearray(12)
        >>> written = b64encode_into(b'hello', output)
        >>> written, bytes(output[:written])
        (8, b'aGVsbG8=')
    """
    ...

def b64decode(
    s: str | ReadableBuffer,
    altchars: str | ReadableBuffer | None = None,
    validate: bool = ...,
    *,
    padded: bool = True,
    ignorechars: ReadableBuffer = ...,
    canonical: bool = False,
) -> bytes:
    """Decode an ASCII string or bytes-like Base64 value.

    By default this follows Python's lenient Base64 behavior. Strict alphabet
    validation, unpadded input, a custom ignored-byte set, and canonical tail
    bit validation are available for protocols with tighter requirements.

    Args:
        s: ASCII text or contiguous bytes-like Base64 data.
        altchars: Two characters replacing "+" and "/", or None for the
            standard alphabet.
        validate: Reject non-alphabet bytes when true. The default is lenient
            unless ignorechars is supplied.
        padded: Require normal padding and quartet alignment when true; accept
            a final unpadded quantum when false.
        ignorechars: Bytes permitted outside the alphabet in lenient mode.
        canonical: Reject non-zero unused bits in the final Base64 quantum.

    Returns:
        Newly allocated decoded bytes.

    Raises:
        binascii.Error: The input has invalid Base64 data, padding, or tail bits.
        TypeError: An argument has an unsupported type.
        ValueError: Text input is not ASCII or altchars is not length two.

    Examples:
        >>> b64decode(b'aGVsbG8=', validate=True)
        b'hello'
        >>> b64decode(b'aGVsbG8', padded=False, canonical=True)
        b'hello'
    """
    ...

def b64decode_batch(
    items: list[str | ReadableBuffer],
    altchars: str | ReadableBuffer | None = None,
    validate: bool = False,
) -> list[bytes]:
    """Decode a list of padded Base64 values.

    Every item uses the same alphabet and validation mode. Results preserve
    input order; an invalid item aborts the operation without a partial list.

    Args:
        items: A list of ASCII strings or bytes-like Base64 values.
        altchars: Two characters replacing "+" and "/", or None for the
            standard alphabet.
        validate: Reject bytes outside the selected alphabet when true.

    Returns:
        One newly allocated decoded byte string per input item.

    Raises:
        binascii.Error: An item contains invalid Base64 data or padding.
        TypeError: items is not a list or an item has an invalid type.
        ValueError: Text is not ASCII or altchars is not length two.

    Examples:
        >>> b64decode_batch([b'b25l', b'dHdv'], validate=True)
        [b'one', b'two']
    """
    ...

def b64decode_batch_into(
    items: list[str | ReadableBuffer],
    outputs: list[bytearray],
    altchars: str | ReadableBuffer | None = None,
    validate: bool = False,
) -> list[int]:
    """Decode each padded Base64 item into a matching reusable bytearray.

    Destinations retain their size and only their written prefixes change.
    Processing is fail-fast and non-transactional: earlier destinations remain
    modified, and the failing destination may be partly written.

    Args:
        items: A list of ASCII strings or bytes-like Base64 values.
        outputs: An equal-length list of distinct destination bytearrays.
        altchars: Two characters replacing "+" and "/", or None for the
            standard alphabet.
        validate: Reject bytes outside the selected alphabet when true.

    Returns:
        The number of decoded bytes written to each destination.

    Raises:
        binascii.Error: An item contains invalid Base64 data or padding.
        TypeError: A container, item, or destination has an invalid type.
        ValueError: The lists differ in length, a destination is repeated or too
            small, text is not ASCII, or altchars is not length two.

    Examples:
        >>> outputs = [bytearray(3), bytearray(3)]
        >>> b64decode_batch_into([b'b25l', b'dHdv'], outputs, validate=True)
        [3, 3]
        >>> [bytes(output) for output in outputs]
        [b'one', b'two']
    """
    ...

def b64decode_into(
    s: str | ReadableBuffer,
    output: bytearray,
    altchars: str | ReadableBuffer | None = None,
    validate: bool = ...,
    *,
    padded: bool = True,
    ignorechars: ReadableBuffer = ...,
    canonical: bool = False,
) -> int:
    """Decode Base64 data into a reusable bytearray.

    The options match b64decode. The destination keeps its size and bytes after
    the returned prefix remain unchanged. On malformed input, part of the
    destination prefix may already have been modified.

    Args:
        s: ASCII text or contiguous bytes-like Base64 data.
        output: Destination bytearray with room for the complete result.
        altchars: Two characters replacing "+" and "/", or None for the
            standard alphabet.
        validate: Reject non-alphabet bytes when true. The default is lenient
            unless ignorechars is supplied.
        padded: Require padding and quartet alignment when true; accept a final
            unpadded quantum when false.
        ignorechars: Bytes permitted outside the alphabet in lenient mode.
        canonical: Reject non-zero unused bits in the final Base64 quantum.

    Returns:
        The number of decoded bytes written to output.

    Raises:
        binascii.Error: The input has invalid Base64 data, padding, or tail bits.
        TypeError: An argument has an unsupported type.
        ValueError: The destination is too small, text is not ASCII, or
            altchars is not length two.

    Examples:
        >>> output = bytearray(8)
        >>> written = b64decode_into(b'aGVsbG8=', output, validate=True)
        >>> written, bytes(output[:written])
        (5, b'hello')
    """
    ...

def standard_b64encode(s: ReadableBuffer) -> bytes:
    """Encode bytes with the padded standard Base64 alphabet.

    Args:
        s: Contiguous bytes-like data to encode.

    Returns:
        Newly allocated Base64 bytes using "+" and "/".

    Raises:
        TypeError: s is not a contiguous bytes-like object.

    Examples:
        >>> standard_b64encode(b'hello')
        b'aGVsbG8='
    """
    ...

def standard_b64encode_into(s: ReadableBuffer, output: bytearray) -> int:
    """Encode bytes with the standard alphabet into a reusable bytearray.

    Args:
        s: Contiguous bytes-like data to encode.
        output: Destination bytearray with room for the padded result.

    Returns:
        The number of bytes written to output.

    Raises:
        TypeError: An argument has an unsupported type.
        ValueError: output is too small.

    Examples:
        >>> output = bytearray(8)
        >>> standard_b64encode_into(b'hello', output)
        8
        >>> bytes(output)
        b'aGVsbG8='
    """
    ...

def standard_b64encode_batch(items: list[ReadableBuffer]) -> list[bytes]:
    """Encode each item with the padded standard Base64 alphabet."""
    ...

def standard_b64encode_batch_into(items: list[ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Encode each item into its matching reusable bytearray."""
    ...

def standard_b64decode_batch(items: list[str | ReadableBuffer]) -> list[bytes]:
    """Decode each item with the padded standard Base64 alphabet."""
    ...

def standard_b64decode_batch_into(items: list[str | ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Decode each item into its matching reusable bytearray."""
    ...

def standard_b64decode(s: str | ReadableBuffer) -> bytes:
    """Decode padded Base64 using the standard alphabet.

    Non-alphabet characters are discarded in the same lenient manner as
    Python's base64.standard_b64decode function.

    Args:
        s: ASCII text or contiguous bytes-like Base64 data.

    Returns:
        Newly allocated decoded bytes.

    Raises:
        binascii.Error: The remaining Base64 data has invalid padding.
        TypeError: s has an unsupported type.
        ValueError: Text input contains non-ASCII characters.

    Examples:
        >>> standard_b64decode(b'aGVsbG8=')
        b'hello'
    """
    ...

def standard_b64decode_into(s: str | ReadableBuffer, output: bytearray) -> int:
    """Decode standard Base64 into a reusable bytearray.

    Args:
        s: ASCII text or contiguous bytes-like Base64 data.
        output: Destination bytearray with room for the decoded result.

    Returns:
        The number of decoded bytes written to output.

    Raises:
        binascii.Error: The input has invalid Base64 padding.
        TypeError: An argument has an unsupported type.
        ValueError: The destination is too small or text is not ASCII.

    Examples:
        >>> output = bytearray(5)
        >>> standard_b64decode_into(b'aGVsbG8=', output)
        5
        >>> bytes(output)
        b'hello'
    """
    ...

def urlsafe_b64encode_batch(items: list[ReadableBuffer]) -> list[bytes]:
    """Encode each item with the padded URL-safe Base64 alphabet."""
    ...

def urlsafe_b64encode_batch_into(items: list[ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Encode each item with the URL-safe alphabet into its matching reusable bytearray."""
    ...

def urlsafe_b64decode_batch(items: list[str | ReadableBuffer]) -> list[bytes]:
    """Decode each item with the padded URL-safe Base64 alphabet."""
    ...

def urlsafe_b64decode_batch_into(items: list[str | ReadableBuffer], outputs: list[bytearray]) -> list[int]:
    """Decode each URL-safe item into its matching reusable bytearray."""
    ...

def urlsafe_b64encode(s: ReadableBuffer, *, padded: bool = True) -> bytes:
    """Encode bytes with the URL-safe Base64 alphabet.

    Args:
        s: Contiguous bytes-like data to encode.
        padded: Append trailing "=" padding when required.

    Returns:
        Newly allocated Base64 bytes using "-" and "_".

    Raises:
        TypeError: s is not a contiguous bytes-like object.

    Examples:
        >>> urlsafe_b64encode(bytes([251, 255]), padded=False)
        b'-_8'
    """
    ...

def urlsafe_b64encode_into(s: ReadableBuffer, output: bytearray, *, padded: bool = True) -> int:
    """Encode bytes with the URL-safe alphabet into a reusable bytearray.

    Args:
        s: Contiguous bytes-like data to encode.
        output: Destination bytearray with room for the complete result.
        padded: Append trailing "=" padding when required.

    Returns:
        The number of bytes written to output.

    Raises:
        TypeError: An argument has an unsupported type.
        ValueError: output is too small.

    Examples:
        >>> output = bytearray(4)
        >>> urlsafe_b64encode_into(bytes([251, 255]), output)
        4
        >>> bytes(output)
        b'-_8='
    """
    ...

def urlsafe_b64decode(s: str | ReadableBuffer, *, padded: bool = ...) -> bytes:
    """Decode Base64 using the URL-safe alphabet.

    The padded default follows CPython: true through Python 3.14 and false from
    Python 3.15 onward.

    Args:
        s: ASCII text or bytes-like URL-safe Base64 data.
        padded: Require padding when true; accept an unpadded tail when false.

    Returns:
        Newly allocated decoded bytes.

    Raises:
        binascii.Error: The input has invalid Base64 data or padding.
        TypeError: s has an unsupported type.
        ValueError: Text input contains non-ASCII characters.

    Examples:
        >>> urlsafe_b64decode(b'-_8=', padded=True)
        b'\\xfb\\xff'
    """
    ...

def urlsafe_b64decode_into(s: str | ReadableBuffer, output: bytearray, *, padded: bool = ...) -> int:
    """Decode URL-safe Base64 into a reusable bytearray.

    The padded default follows CPython: true through Python 3.14 and false from
    Python 3.15 onward.

    Args:
        s: ASCII text or bytes-like URL-safe Base64 data.
        output: Destination bytearray with room for the decoded result.
        padded: Require padding when true; accept an unpadded tail when false.

    Returns:
        The number of decoded bytes written to output.

    Raises:
        binascii.Error: The input has invalid Base64 data or padding.
        TypeError: An argument has an unsupported type.
        ValueError: The destination is too small or text is not ASCII.

    Examples:
        >>> output = bytearray(2)
        >>> urlsafe_b64decode_into(b'-_8=', output, padded=True)
        2
        >>> bytes(output)
        b'\\xfb\\xff'
    """
    ...

def murmur3_32(s: ReadableBuffer, seed: int = 0) -> int:
    """Compute the canonical MurmurHash3 x86 32-bit hash.

    MurmurHash3 is a fast non-cryptographic hash. The result is compatible with
    the original x86-32 reference algorithm.

    Args:
        s: Contiguous bytes-like data to hash.
        seed: Initial unsigned 32-bit seed.

    Returns:
        The unsigned 32-bit hash as a Python integer.

    Raises:
        TypeError: s is not bytes-like or seed is not an integer.
        OverflowError: seed is outside 0 <= seed < 2**32.

    Examples:
        >>> hex(murmur3_32(b'hello'))
        '0x248bfa47'
    """
    ...

def murmur3_x86_128_digest(s: ReadableBuffer, seed: int = 0) -> bytes:
    """Compute the canonical MurmurHash3 x86 128-bit digest.

    The four result words are serialized as little-endian 32-bit integers.

    Args:
        s: Contiguous bytes-like data to hash.
        seed: Initial unsigned 32-bit seed.

    Returns:
        A 16-byte digest.

    Raises:
        TypeError: s is not bytes-like or seed is not an integer.
        OverflowError: seed is outside 0 <= seed < 2**32.

    Examples:
        >>> len(murmur3_x86_128_digest(b'hello'))
        16
    """
    ...

def murmur3_x64_128_digest(s: ReadableBuffer, seed: int = 0) -> bytes:
    """Compute the canonical MurmurHash3 x64 128-bit digest.

    The two result words are serialized as little-endian 64-bit integers.

    Args:
        s: Contiguous bytes-like data to hash.
        seed: Initial unsigned 32-bit seed.

    Returns:
        A 16-byte digest.

    Raises:
        TypeError: s is not bytes-like or seed is not an integer.
        OverflowError: seed is outside 0 <= seed < 2**32.

    Examples:
        >>> len(murmur3_x64_128_digest(b'hello'))
        16
    """
    ...

def xxh3_64(s: ReadableBuffer, seed: int = 0) -> int:
    """Compute the canonical XXH3 64-bit hash.

    XXH3 is a non-cryptographic hash designed for speed.

    Args:
        s: Contiguous bytes-like data to hash.
        seed: Initial unsigned 64-bit seed.

    Returns:
        The unsigned 64-bit hash as a Python integer.

    Raises:
        TypeError: s is not bytes-like or seed is not an integer.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> hex(xxh3_64(b''))
        '0x2d06800538d394c2'
    """
    ...

def xxh3_128(s: ReadableBuffer, seed: int = 0) -> int:
    """Compute the canonical XXH3 128-bit hash.

    The low 64-bit word occupies the least significant half of the returned
    integer. XXH3 is non-cryptographic.

    Args:
        s: Contiguous bytes-like data to hash.
        seed: Initial unsigned 64-bit seed.

    Returns:
        The unsigned 128-bit hash as a Python integer.

    Raises:
        TypeError: s is not bytes-like or seed is not an integer.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> hex(xxh3_128(b''))
        '0x99aa06d3014798d86001c324468d497f'
    """
    ...

def xxh3_64_batch(items: list[ReadableBuffer], seed: int = 0) -> list[int]:
    """Compute canonical XXH3 64-bit hashes for a list of inputs.

    Args:
        items: A list of contiguous bytes-like objects to hash.
        seed: Initial unsigned 64-bit seed shared by every item.

    Returns:
        One unsigned 64-bit integer per item, in input order.

    Raises:
        TypeError: The container, an item, or seed has an invalid type.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> xxh3_64_batch([b'', b'hello']) == [xxh3_64(b''), xxh3_64(b'hello')]
        True
    """
    ...

def xxh3_64_batch_into(items: list[ReadableBuffer], output: bytearray, seed: int = 0) -> int:
    """Write XXH3 64-bit hashes as packed little-endian bytes.

    Inputs and capacity are validated before output is mutated. Bytes after the
    written prefix remain unchanged.

    Args:
        items: A list of contiguous bytes-like objects to hash.
        output: Destination with at least 8 * len(items) bytes.
        seed: Initial unsigned 64-bit seed shared by every item.

    Returns:
        The total number of bytes written.

    Raises:
        TypeError: A container, item, destination, or seed has an invalid type.
        ValueError: output is too small.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> output = bytearray(8)
        >>> xxh3_64_batch_into([b'hello'], output)
        8
        >>> int.from_bytes(output, 'little') == xxh3_64(b'hello')
        True
    """
    ...

def xxh3_128_batch(items: list[ReadableBuffer], seed: int = 0) -> list[int]:
    """Compute canonical XXH3 128-bit hashes for a list of inputs.

    Args:
        items: A list of contiguous bytes-like objects to hash.
        seed: Initial unsigned 64-bit seed shared by every item.

    Returns:
        One unsigned 128-bit integer per item, in input order.

    Raises:
        TypeError: The container, an item, or seed has an invalid type.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> xxh3_128_batch([b'', b'hello']) == [xxh3_128(b''), xxh3_128(b'hello')]
        True
    """
    ...

def xxh3_128_batch_into(items: list[ReadableBuffer], output: bytearray, seed: int = 0) -> int:
    """Write XXH3 128-bit hashes as packed little-endian bytes.

    Inputs and capacity are validated before output is mutated. Bytes after the
    written prefix remain unchanged.

    Args:
        items: A list of contiguous bytes-like objects to hash.
        output: Destination with at least 16 * len(items) bytes.
        seed: Initial unsigned 64-bit seed shared by every item.

    Returns:
        The total number of bytes written.

    Raises:
        TypeError: A container, item, destination, or seed has an invalid type.
        ValueError: output is too small.
        OverflowError: seed is outside 0 <= seed < 2**64.

    Examples:
        >>> output = bytearray(16)
        >>> xxh3_128_batch_into([b'hello'], output)
        16
        >>> int.from_bytes(output, 'little') == xxh3_128(b'hello')
        True
    """
    ...

class murmur3_x86_32:
    """Incremental MurmurHash3 x86 32-bit hasher.

    Args:
        data: Optional initial contiguous bytes-like data.
        seed: Initial unsigned 32-bit seed.

    Examples:
        >>> hasher = murmur3_x86_32(b'hello', seed=7)
        >>> hasher.update(b' world')
        >>> hasher.hexdigest() == hasher.digest().hex()
        True
    """

    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None:
        """Initialize an incremental x86 32-bit hash state.

        Args:
            data: Optional initial contiguous bytes-like data.
            seed: Initial unsigned 32-bit seed.

        Raises:
            TypeError: data is not bytes-like or seed is not an integer.
            OverflowError: seed is outside 0 <= seed < 2**32.
        """
        ...

    def update(self, data: ReadableBuffer) -> None:
        """Append bytes to the hash state.

        Args:
            data: Contiguous bytes-like data to append.

        Returns:
            None.

        Raises:
            TypeError: data is not bytes-like.

        Examples:
            >>> hasher = murmur3_x86_32()
            >>> hasher.update(b'hello')
        """
        ...

    def digest(self) -> bytes:
        """Return the current digest without consuming the state.

        Returns:
            A four-byte little-endian digest.

        Examples:
            >>> len(murmur3_x86_32(b'hello').digest())
            4
        """
        ...

    def hexdigest(self) -> str:
        """Return the current digest as lowercase hexadecimal text.

        Returns:
            An eight-character hexadecimal string.

        Examples:
            >>> murmur3_x86_32(b'hello').hexdigest()
            '47fa8b24'
        """
        ...

    def copy(self) -> murmur3_x86_32:
        """Return an independent copy of the current hash state.

        Returns:
            A hasher with the same state.

        Examples:
            >>> original = murmur3_x86_32(b'prefix')
            >>> original.copy().digest() == original.digest()
            True
        """
        ...

    @property
    def digest_size(self) -> int:
        """Return the digest size in bytes (4)."""
        ...

    @property
    def block_size(self) -> int:
        """Return the algorithm block size in bytes (4)."""
        ...

    @property
    def name(self) -> str:
        """Return the algorithm name (murmur3_x86_32)."""
        ...

class murmur3_x86_128:
    """Incremental MurmurHash3 x86 128-bit hasher.

    Args:
        data: Optional initial contiguous bytes-like data.
        seed: Initial unsigned 32-bit seed.

    Examples:
        >>> hasher = murmur3_x86_128(b'hello', seed=7)
        >>> hasher.update(b' world')
        >>> len(hasher.digest())
        16
    """

    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None:
        """Initialize an incremental x86 128-bit hash state.

        Args:
            data: Optional initial contiguous bytes-like data.
            seed: Initial unsigned 32-bit seed.

        Raises:
            TypeError: data is not bytes-like or seed is not an integer.
            OverflowError: seed is outside 0 <= seed < 2**32.
        """
        ...

    def update(self, data: ReadableBuffer) -> None:
        """Append bytes to the hash state.

        Args:
            data: Contiguous bytes-like data to append.

        Returns:
            None.

        Raises:
            TypeError: data is not bytes-like.

        Examples:
            >>> hasher = murmur3_x86_128()
            >>> hasher.update(b'hello')
        """
        ...

    def digest(self) -> bytes:
        """Return the current digest without consuming the state.

        Returns:
            A 16-byte digest of four little-endian 32-bit words.

        Examples:
            >>> len(murmur3_x86_128(b'hello').digest())
            16
        """
        ...

    def hexdigest(self) -> str:
        """Return the current digest as lowercase hexadecimal text.

        Returns:
            A 32-character hexadecimal string.

        Examples:
            >>> len(murmur3_x86_128(b'hello').hexdigest())
            32
        """
        ...

    def copy(self) -> murmur3_x86_128:
        """Return an independent copy of the current hash state.

        Returns:
            A hasher with the same state.

        Examples:
            >>> original = murmur3_x86_128(b'prefix')
            >>> original.copy().digest() == original.digest()
            True
        """
        ...

    @property
    def digest_size(self) -> int:
        """Return the digest size in bytes (16)."""
        ...

    @property
    def block_size(self) -> int:
        """Return the algorithm block size in bytes (16)."""
        ...

    @property
    def name(self) -> str:
        """Return the algorithm name (murmur3_x86_128)."""
        ...

class murmur3_x64_128:
    """Incremental MurmurHash3 x64 128-bit hasher.

    Args:
        data: Optional initial contiguous bytes-like data.
        seed: Initial unsigned 32-bit seed.

    Examples:
        >>> hasher = murmur3_x64_128(b'hello', seed=7)
        >>> checkpoint = hasher.copy()
        >>> hasher.update(b' world')
        >>> hasher.digest() != checkpoint.digest()
        True
    """

    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None:
        """Initialize an incremental x64 128-bit hash state.

        Args:
            data: Optional initial contiguous bytes-like data.
            seed: Initial unsigned 32-bit seed.

        Raises:
            TypeError: data is not bytes-like or seed is not an integer.
            OverflowError: seed is outside 0 <= seed < 2**32.
        """
        ...

    def update(self, data: ReadableBuffer) -> None:
        """Append bytes to the hash state.

        Args:
            data: Contiguous bytes-like data to append.

        Returns:
            None.

        Raises:
            TypeError: data is not bytes-like.

        Examples:
            >>> hasher = murmur3_x64_128()
            >>> hasher.update(b'hello')
        """
        ...

    def digest(self) -> bytes:
        """Return the current digest without consuming the state.

        Returns:
            A 16-byte digest of two little-endian 64-bit words.

        Examples:
            >>> len(murmur3_x64_128(b'hello').digest())
            16
        """
        ...

    def hexdigest(self) -> str:
        """Return the current digest as lowercase hexadecimal text.

        Returns:
            A 32-character hexadecimal string.

        Examples:
            >>> len(murmur3_x64_128(b'hello').hexdigest())
            32
        """
        ...

    def copy(self) -> murmur3_x64_128:
        """Return an independent copy of the current hash state.

        Returns:
            A hasher with the same state.

        Examples:
            >>> original = murmur3_x64_128(b'prefix')
            >>> original.copy().digest() == original.digest()
            True
        """
        ...

    @property
    def digest_size(self) -> int:
        """Return the digest size in bytes (16)."""
        ...

    @property
    def block_size(self) -> int:
        """Return the algorithm block size in bytes (16)."""
        ...

    @property
    def name(self) -> str:
        """Return the algorithm name (murmur3_x64_128)."""
        ...
