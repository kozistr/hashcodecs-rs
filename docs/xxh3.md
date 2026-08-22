# XXH3

XXH3 is a very fast non-cryptographic hash from the xxHash family. `hashcodecs` implements canonical XXH3-64 and
XXH3-128 hashes, including efficient native batching.

> Warning: XXH3 is designed for speed and distribution, not cryptographic security. Do not use it as a MAC, a
> password hash, or an integrity boundary against malicious input.

## One Value

`xxh3_64(s, seed=0)` and `xxh3_128(s, seed=0)` return unsigned Python integers. The seed must fit in an unsigned
64-bit integer.

```python
from hashcodecs import xxh3_64, xxh3_128

assert xxh3_64(b'') == 0x2D06800538D394C2
assert xxh3_128(b'') == 0x99AA06D3014798D86001C324468D497F

key = xxh3_64(b'cache-key', seed=42)
```

The returned integers are the canonical XXH3 numerical values. Convert a result to a serialized representation only
at your protocol boundary, and specify the byte order there.

## Native Batches

`xxh3_64_batch(items, seed=0)` and `xxh3_128_batch(items, seed=0)` accept a `list` of bytes-like objects and return
a list of integer digests in the same order.

```python
from hashcodecs import xxh3_64_batch

digests = xxh3_64_batch([b'first', b'second', b'third'], seed=42)
```

For a long, equal-sized collection, batching lets the extension reuse setup work and select its vectorized batch
backend where supported. Mixed lengths are fully supported and retain canonical results.

## Packed Outputs

For binary pipelines, `xxh3_64_batch_into` and `xxh3_128_batch_into` pack each digest into one output
`bytearray`. They return the total number of bytes written.

```python
from hashcodecs import xxh3_64_batch, xxh3_64_batch_into

items = [b'first', b'second']
output = bytearray(8 * len(items))
written = xxh3_64_batch_into(items, output, seed=42)

assert written == 16
assert output == b''.join(value.to_bytes(8, 'little') for value in xxh3_64_batch(items, seed=42))
```

Use 8 bytes per item for the 64-bit API and 16 bytes per item for the 128-bit API. Packed digests are **little
endian**. The function validates every input and destination capacity before mutating the output, and it permits the
output bytearray to also be one of the inputs.

See the [XXH3 API Reference](api/xxh3.md) for the complete runtime signatures and docstrings.
