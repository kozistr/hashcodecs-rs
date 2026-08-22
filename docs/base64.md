# Base64

`hashcodecs.base64` is a fast, API-compatible subset of Python's `base64` module. Its single-value operations add
controls for unpadded input, canonical validation, line wrapping, batching, and caller-owned outputs.

```python
import hashcodecs.base64 as base64

encoded = base64.b64encode(b'hello')
assert encoded == b'aGVsbG8='
assert base64.b64decode(encoded, validate=True) == b'hello'
```

## Encodings

`b64encode(s, altchars=None, *, padded=True, wrapcol=0)` returns standard Base64 by default. Supply a two-byte
`altchars` value to replace `+` and `/`; `b'-_'` produces the URL-safe alphabet.

```python
assert base64.b64encode(b'\xfb\xff') == b'+/8='
assert base64.b64encode(b'\xfb\xff', b'-_') == b'-_8='
assert base64.urlsafe_b64encode(b'\xfb\xff', padded=False) == b'-_8'
```

Set `padded=False` when an external protocol requires padding-free output. `wrapcol` inserts newlines after the
requested column width; its default of `0` leaves output unwrapped.

The `standard_b64encode` and `urlsafe_b64encode` helpers select the respective alphabets. Their `*_into` and
`*_batch` counterparts follow the same convention.

## Decoding and Validation

`b64decode(s, altchars=None, validate=False, *, padded=True, ignorechars=..., canonical=False)` decodes an ASCII
string or bytes-like value.

- `validate=True` rejects characters outside the chosen alphabet instead of discarding them.
- `padded=False` accepts an unpadded final Base64 quantum.
- `ignorechars` specifies which non-alphabet bytes lenient decoding may ignore. Its default preserves the standard
  library's lenient behavior.
- `canonical=True` rejects encodings with non-zero unused tail bits, so each payload has exactly one accepted
  representation.

```python
assert base64.b64decode(b'Y W\nJj') == b'abc'
assert base64.b64decode(b'YWJj', validate=True) == b'abc'
assert base64.b64decode(b'-_8', b'-_', padded=False, validate=True) == b'\xfb\xff'

# Canonical validation is useful when encoded values are compared or signed.
assert base64.b64decode(b'AA', padded=False, canonical=True) == b'\x00'
```

Malformed input raises `binascii.Error`; invalid types and option values raise the corresponding Python type or
value errors. The URL-safe decoder's default padding behavior follows the running CPython version, matching its
standard-library API.

## Reusable Outputs

`b64encode_into` and `b64decode_into` write to a caller-supplied `bytearray` and return the number of bytes
written. Bytes after the returned length are left unchanged.

```python
payload = b'hello'
encoded = bytearray(4 * ((len(payload) + 2) // 3))
written = base64.b64encode_into(payload, encoded)
assert encoded[:written] == b'aGVsbG8='

decoded = bytearray(len(payload))
written = base64.b64decode_into(encoded, decoded, validate=True)
assert decoded[:written] == payload
```

The destination must be large enough. For padded encoding, allocate `4 * ((len(data) + 2) // 3)` bytes. Decoding
can never produce more bytes than the encoded input length, so allocating that length is a simple conservative
choice.

## Batches

`b64encode_batch(items, altchars=None)` and `b64decode_batch(items, altchars=None, validate=False)` process a
list in one call and return a list of byte strings. `*_batch_into` writes each result to a matching list of distinct
`bytearray` destinations and returns a list of written lengths.

```python
values = [b'one', b'two']
outputs = [bytearray(4), bytearray(4)]
lengths = base64.b64encode_batch_into(values, outputs)
assert [output[:length] for output, length in zip(outputs, lengths)] == [b'b25l', b'dHdv']
```

Batch destinations are checked for count, type, distinctness, and capacity before their own item is written. A
batch therefore stops at the first invalid item; items completed earlier remain written.

See the [Base64 API Reference](api/base64.md) for the complete runtime signatures and docstrings.
