# MurmurHash3

MurmurHash3 is a fast non-cryptographic hash family. `hashcodecs` exposes the x86 32-bit, x86 128-bit, and x64
128-bit variants with one-shot and `hashlib`-style incremental interfaces.

> Warning: MurmurHash3 is not a cryptographic hash. Do not use it for passwords, signatures, or adversarial input
> where collision resistance is required.

## One-Shot Hashing

All one-shot functions take a bytes-like value and an optional unsigned 32-bit `seed`.

```python
from hashcodecs import murmur3_32, murmur3_x64_128_digest, murmur3_x86_128_digest

assert murmur3_32(b'hello') == 0x248BFA47
assert murmur3_x86_128_digest(bytes([1, 2, 3])) == bytes.fromhex('e16401f6334213b5334213b5334213b5')
assert murmur3_x64_128_digest(bytes([1, 2, 3])) == bytes.fromhex('a937130eef3e641a659a233c404a4e49')
```

| Function | Result |
| --- | --- |
| `murmur3_32(s, seed=0)` | Unsigned 32-bit `int` from the x86-32 algorithm. |
| `murmur3_x86_128_digest(s, seed=0)` | 16-byte x86-128 digest. |
| `murmur3_x64_128_digest(s, seed=0)` | 16-byte x64-128 digest. |

Use the same variant and seed in every system that needs matching digest values. A negative seed or one larger
than `2**32 - 1` raises `OverflowError`.

## Incremental Hashing

`murmur3_x86_32`, `murmur3_x86_128`, and `murmur3_x64_128` accept optional initial data and a seed. They expose
the standard `hashlib` workflow: `update`, `digest`, `hexdigest`, and `copy`.

```python
from hashcodecs import murmur3_x64_128

hasher = murmur3_x64_128(seed=42)
hasher.update(b'hello')
checkpoint = hasher.copy()
hasher.update(b' world')

assert checkpoint.hexdigest() == checkpoint.digest().hex()
assert hasher.digest() != checkpoint.digest()
```

Calling `digest()` is non-destructive: more data can be added afterward. Each hasher also reports `name`,
`digest_size`, and `block_size`.

| Hasher | Digest size | Block size |
| --- | ---: | ---: |
| `murmur3_x86_32` | 4 bytes | 4 bytes |
| `murmur3_x86_128` | 16 bytes | 16 bytes |
| `murmur3_x64_128` | 16 bytes | 16 bytes |
