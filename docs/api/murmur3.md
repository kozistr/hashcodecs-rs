# MurmurHash3 API Reference

The functions and incremental hashers below are rendered from the public API declarations.

::: hashcodecs._hashcodecs
    options:
      members:
        - murmur3_32
        - murmur3_x86_128_digest
        - murmur3_x64_128_digest
        - murmur3_x86_32
        - murmur3_x86_128
        - murmur3_x64_128
      show_root_heading: false

## Parameters

- s: One bytes-like value to hash.
- data: Optional initial bytes for an incremental hasher, or bytes appended by update.
- seed: Optional integer seed. Use the same seed to reproduce a hash.

## Examples

```python
from hashcodecs.murmur3 import (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
)

murmur3_32(b'hello', seed=7)
murmur3_x86_128_digest(b'hello', seed=7)
murmur3_x64_128_digest(b'hello', seed=7)

x86_32 = murmur3_x86_32(seed=7)
x86_32.update(b'hello')
x86_32.hexdigest()

x86_128 = murmur3_x86_128(b'hello', seed=7)
x86_128.digest()

x64_128 = murmur3_x64_128(seed=7)
x64_128.update(b'hello')
x64_128.copy().digest()
```
