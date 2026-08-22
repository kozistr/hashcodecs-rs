# Base64 API Reference

The functions below are rendered from the public API declarations. They describe the exact call signatures and
behavior exposed by `hashcodecs.base64`.

::: hashcodecs._hashcodecs
    options:
      members:
        - b64encode
        - b64encode_batch
        - b64encode_batch_into
        - b64encode_into
        - b64decode
        - b64decode_batch
        - b64decode_batch_into
        - b64decode_into
        - standard_b64encode
        - standard_b64encode_into
        - standard_b64decode
        - standard_b64decode_into
        - urlsafe_b64encode
        - urlsafe_b64encode_into
        - urlsafe_b64decode
        - urlsafe_b64decode_into
      show_root_heading: false

## Standard and URL-safe batch helpers

::: hashcodecs.base64
    options:
      members:
        - standard_b64encode_batch
        - standard_b64encode_batch_into
        - standard_b64decode_batch
        - standard_b64decode_batch_into
        - urlsafe_b64encode_batch
        - urlsafe_b64encode_batch_into
        - urlsafe_b64decode_batch
        - urlsafe_b64decode_batch_into
      show_root_heading: false

## Parameters

- s: A single bytes-like input, or a Base64 string when decoding.
- items: Inputs processed in order by a batch helper.
- altchars: Two replacement characters for the standard Base64 alphabet.
- padded: Preserve or require trailing Base64 padding.
- wrapcol: Wrap encoded output at this column width; zero disables wrapping.
- validate: Reject invalid Base64 input when true.
- ignorechars: Bytes allowed in non-validating decode input.
- canonical: Require canonical Base64 bits when true.
- output and outputs: Preallocated bytearray destinations for into helpers.

## Examples

```python
from hashcodecs import base64

base64.b64encode(b'hello')
base64.b64encode_batch([b'one', b'two'])
encoded = bytearray(32)
base64.b64encode_into(b'hello', encoded)
encoded_items = [bytearray(32), bytearray(32)]
base64.b64encode_batch_into([b'one', b'two'], encoded_items)
base64.b64decode(b'aGVsbG8=')
base64.b64decode_batch([b'b25l', b'dHdv'])
decoded = bytearray(32)
base64.b64decode_into(b'aGVsbG8=', decoded)
decoded_items = [bytearray(32), bytearray(32)]
base64.b64decode_batch_into([b'b25l', b'dHdv'], decoded_items)

base64.standard_b64encode(b'hello')
base64.standard_b64encode_into(b'hello', encoded)
base64.standard_b64encode_batch([b'one', b'two'])
base64.standard_b64encode_batch_into([b'one', b'two'], encoded_items)
base64.standard_b64decode(b'aGVsbG8=')
base64.standard_b64decode_into(b'aGVsbG8=', decoded)
base64.standard_b64decode_batch([b'b25l', b'dHdv'])
base64.standard_b64decode_batch_into([b'b25l', b'dHdv'], decoded_items)

base64.urlsafe_b64encode(b'hello')
base64.urlsafe_b64encode_into(b'hello', encoded)
base64.urlsafe_b64encode_batch([b'one', b'two'])
base64.urlsafe_b64encode_batch_into([b'one', b'two'], encoded_items)
base64.urlsafe_b64decode(b'aGVsbG8=')
base64.urlsafe_b64decode_into(b'aGVsbG8=', decoded)
base64.urlsafe_b64decode_batch([b'b25l', b'dHdv'])
base64.urlsafe_b64decode_batch_into([b'b25l', b'dHdv'], decoded_items)
```
