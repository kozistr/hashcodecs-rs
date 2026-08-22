from _typeshed import ReadableBuffer

def b64encode(
    s: ReadableBuffer,
    altchars: ReadableBuffer | None = None,
    *,
    padded: bool = True,
    wrapcol: int = 0,
) -> bytes:
    """Encode bytes as Base64 with optional alternate characters, padding, and line wrapping."""
    ...

def b64encode_batch(items: list[ReadableBuffer], altchars: ReadableBuffer | None = None) -> list[bytes]:
    """Encode each input in order and return one Base64 value per item."""
    ...

def b64encode_batch_into(
    items: list[ReadableBuffer],
    outputs: list[bytearray],
    altchars: ReadableBuffer | None = None,
) -> list[int]:
    """Encode each input into the matching output buffer and return bytes written per item."""
    ...

def b64encode_into(
    s: ReadableBuffer,
    output: bytearray,
    altchars: ReadableBuffer | None = None,
    *,
    padded: bool = True,
    wrapcol: int = 0,
) -> int:
    """Encode bytes into a caller-owned bytearray and return the number of bytes written."""
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
    """Decode Base64 with optional strict, unpadded, and canonical validation modes."""
    ...

def b64decode_batch(
    items: list[str | ReadableBuffer],
    altchars: str | ReadableBuffer | None = None,
    validate: bool = False,
) -> list[bytes]:
    """Decode each input in order and return one bytes value per item."""
    ...

def b64decode_batch_into(
    items: list[str | ReadableBuffer],
    outputs: list[bytearray],
    altchars: str | ReadableBuffer | None = None,
    validate: bool = False,
) -> list[int]:
    """Decode each input into the matching output buffer and return bytes written per item."""
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
    """Decode Base64 into a caller-owned bytearray and return the number of bytes written."""
    ...

def standard_b64encode(s: ReadableBuffer) -> bytes:
    """Encode bytes with the standard +/ Base64 alphabet."""
    ...

def standard_b64encode_into(s: ReadableBuffer, output: bytearray) -> int:
    """Encode with the standard alphabet into a caller-owned bytearray."""
    ...

def standard_b64decode(s: str | ReadableBuffer) -> bytes:
    """Decode a value that uses the standard +/ Base64 alphabet."""
    ...

def standard_b64decode_into(s: str | ReadableBuffer, output: bytearray) -> int:
    """Decode a standard Base64 value into a caller-owned bytearray."""
    ...

def urlsafe_b64encode(s: ReadableBuffer, *, padded: bool = True) -> bytes:
    """Encode bytes with the URL-safe -_ Base64 alphabet."""
    ...

def urlsafe_b64encode_into(s: ReadableBuffer, output: bytearray, *, padded: bool = True) -> int:
    """Encode with the URL-safe alphabet into a caller-owned bytearray."""
    ...

def urlsafe_b64decode(s: str | ReadableBuffer, *, padded: bool = ...) -> bytes:
    """Decode a value that uses the URL-safe -_ Base64 alphabet."""
    ...

def urlsafe_b64decode_into(s: str | ReadableBuffer, output: bytearray, *, padded: bool = ...) -> int:
    """Decode a URL-safe Base64 value into a caller-owned bytearray."""
    ...

def murmur3_32(s: ReadableBuffer, seed: int = 0) -> int:
    """Return the unsigned 32-bit x86 MurmurHash3 value for bytes-like input."""
    ...

def murmur3_x86_128_digest(s: ReadableBuffer, seed: int = 0) -> bytes:
    """Return the 16-byte x86-128 MurmurHash3 digest for bytes-like input."""
    ...

def murmur3_x64_128_digest(s: ReadableBuffer, seed: int = 0) -> bytes:
    """Return the 16-byte x64-128 MurmurHash3 digest for bytes-like input."""
    ...

def xxh3_64(s: ReadableBuffer, seed: int = 0) -> int:
    """Return the canonical unsigned 64-bit XXH3 hash for bytes-like input."""
    ...

def xxh3_128(s: ReadableBuffer, seed: int = 0) -> int:
    """Return the canonical unsigned 128-bit XXH3 hash for bytes-like input."""
    ...

def xxh3_64_batch(items: list[ReadableBuffer], seed: int = 0) -> list[int]:
    """Return canonical unsigned 64-bit XXH3 hashes for a list in input order."""
    ...

def xxh3_64_batch_into(items: list[ReadableBuffer], output: bytearray, seed: int = 0) -> int:
    """Write 64-bit XXH3 hashes as packed little-endian bytes and return bytes written."""
    ...

def xxh3_128_batch(items: list[ReadableBuffer], seed: int = 0) -> list[int]:
    """Return canonical unsigned 128-bit XXH3 hashes for a list in input order."""
    ...

def xxh3_128_batch_into(items: list[ReadableBuffer], output: bytearray, seed: int = 0) -> int:
    """Write 128-bit XXH3 hashes as packed little-endian bytes and return bytes written."""
    ...

class murmur3_x86_32:
    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None: ...
    def update(self, data: ReadableBuffer) -> None: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def copy(self) -> murmur3_x86_32: ...
    @property
    def digest_size(self) -> int: ...
    @property
    def block_size(self) -> int: ...
    @property
    def name(self) -> str: ...

class murmur3_x86_128:
    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None: ...
    def update(self, data: ReadableBuffer) -> None: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def copy(self) -> murmur3_x86_128: ...
    @property
    def digest_size(self) -> int: ...
    @property
    def block_size(self) -> int: ...
    @property
    def name(self) -> str: ...

class murmur3_x64_128:
    def __init__(self, data: ReadableBuffer | None = None, seed: int = 0) -> None: ...
    def update(self, data: ReadableBuffer) -> None: ...
    def digest(self) -> bytes: ...
    def hexdigest(self) -> str: ...
    def copy(self) -> murmur3_x64_128: ...
    @property
    def digest_size(self) -> int: ...
    @property
    def block_size(self) -> int: ...
    @property
    def name(self) -> str: ...
