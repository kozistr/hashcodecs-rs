# XXH3 API Reference

The functions below are rendered from the public API declarations.

::: hashcodecs._hashcodecs
    options:
      members:
        - xxh3_64
        - xxh3_128
        - xxh3_64_batch
        - xxh3_64_batch_into
        - xxh3_128_batch
        - xxh3_128_batch_into
      show_root_heading: false

## Parameters

- s: One bytes-like value to hash.
- items: Bytes-like values to hash in input order.
- seed: Optional integer seed. Use the same seed to reproduce a hash.
- output: A bytearray receiving little-endian packed batch hashes.

## Examples

```python
from hashcodecs.xxhash import (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
)

xxh3_64(b'hello', seed=7)
xxh3_128(b'hello', seed=7)
xxh3_64_batch([b'one', b'two'])
xxh3_128_batch([b'one', b'two'])

packed_64 = bytearray(16)
xxh3_64_batch_into([b'one', b'two'], packed_64)

packed_128 = bytearray(32)

xxh3_128_batch_into([b'one', b'two'], packed_128)
```
