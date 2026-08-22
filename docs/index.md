# hashcodecs

`hashcodecs` provides SIMD-accelerated Base64, MurmurHash3, and XXH3 for Python. It keeps the familiar
buffer-oriented APIs while providing native batch operations and reusable outputs for allocation-sensitive code.

## Install

```sh
python -m pip install hashcodecs
```

The package supports CPython 3.10 through 3.15 on Linux, macOS, and Windows. The wheel selects the fastest
available implementation at runtime and falls back to portable scalar code when a SIMD feature is unavailable.

## Quick Start

```python
import hashcodecs.base64 as base64
from hashcodecs import murmur3_32, xxh3_64, xxh3_128_batch

assert base64.b64encode(b'hello') == b'aGVsbG8='
assert base64.b64decode(b'aGVsbG8=', validate=True) == b'hello'
assert base64.urlsafe_b64encode(b'hello', padded=False) == b'aGVsbG8'

assert murmur3_32(b'hello') == 0x248BFA47
assert xxh3_64(b'') == 0x2D06800538D394C2
assert xxh3_128_batch([b'hello', b'world']) == [
    0xB5E9C1AD071B3E7FC779CFAA5E523818,
    0xFA0D38A9B38280D0891E4985BDB2583E,
]
```

## Choose an API

| Need | API |
| --- | --- |
| Encode or decode one value | [Base64](base64.md) |
| Compute a stable 32-bit or 128-bit MurmurHash3 digest | [MurmurHash3](murmur3.md) |
| Compute a fast canonical 64-bit or 128-bit XXH3 hash | [XXH3](xxh3.md) |
| Avoid allocating output buffers repeatedly | `*_into` APIs |
| Hash many values in one native call | `*_batch` APIs |

All byte-oriented APIs accept `bytes`, `bytearray`, and `memoryview`. Base64 decoders also accept ASCII `str`.

## Rust

The repository also contains the Rust library used by the Python extension. It is not published on crates.io; use
the repository source when integrating it from Rust.

```rust
let encoded = hashcodecs::b64encode(b"hello");
assert_eq!(encoded, "aGVsbG8=");

assert_eq!(hashcodecs::murmur3_x86_32(b"hello", 0), 0x248b_fa47);
assert_eq!(hashcodecs::xxh3_64(b"", 0), 0x2d06_8005_38d3_94c2);
```

See [Architecture](ARCHITECTURE.md) for dispatch, binding, and safety details, and [Performance](performance.md)
for benchmark methodology and results.
