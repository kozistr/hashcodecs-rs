use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::sync::Once;

use super::add_methods;
use super::schema::*;

const BINDING_COUNT: usize = 24;
static mut METHODS: [ffi::PyMethodDef; BINDING_COUNT + 1] =
    [const { ffi::PyMethodDef::zeroed() }; BINDING_COUNT + 1];

unsafe fn initialize_methods(methods: *mut ffi::PyMethodDef, version: (u8, u8)) {
    let mut method_count = 0;
    macro_rules! register {
        ($binding:ident, $documentation:expr) => {
            assert!(method_count < BINDING_COUNT, "Base64 method table overflow");
            unsafe { $binding.register(methods, &mut method_count, version, $documentation) };
        };
    }
    register!(
        STANDARD_B64ENCODE,
        cr###"standard_b64encode($module, /, s)
--

Encode bytes with the padded standard Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.

Returns:
    Newly allocated Base64 bytes using "+" and "/".

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> standard_b64encode(b'hello')
    b'aGVsbG8='"###
    );
    register!(
        STANDARD_B64ENCODE_INTO,
        cr###"standard_b64encode_into($module, /, s, output)
--

Encode bytes with the standard alphabet into a reusable bytearray.

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
    b'aGVsbG8='"###
    );
    register!(
        URLSAFE_B64ENCODE,
        cr###"urlsafe_b64encode($module, /, s, *, padded=True)
--

Encode bytes with the URL-safe Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.
    padded: Append trailing "=" padding when required.

Returns:
    Newly allocated Base64 bytes using "-" and "_".

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> urlsafe_b64encode(bytes([251, 255]), padded=False)
    b'-_8'"###
    );
    register!(
        URLSAFE_B64ENCODE_INTO,
        cr###"urlsafe_b64encode_into($module, /, s, output, *, padded=True)
--

Encode bytes with the URL-safe alphabet into a reusable bytearray.

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
    b'-_8='"###
    );
    register!(
        B64ENCODE,
        cr###"b64encode($module, /, s, altchars=None, *, padded=True, wrapcol=0)
--

Encode a bytes-like object as Base64.

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
    b'aGVs\nbG8'"###
    );
    register!(
        B64ENCODE_BATCH,
        cr###"b64encode_batch($module, /, items, altchars=None)
--

Encode a list of bytes-like objects as padded Base64.

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
    [b'b25l', b'dHdv']"###
    );
    register!(
        B64ENCODE_BATCH_INTO,
        cr###"b64encode_batch_into($module, /, items, outputs, altchars=None)
--

Encode each item into a matching reusable bytearray.

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
    [b'b25l', b'dHdv']"###
    );
    register!(
        B64ENCODE_INTO,
        cr###"b64encode_into($module, /, s, output, altchars=None, *, padded=True, wrapcol=0)
--

Encode a bytes-like object as Base64 into a reusable bytearray.

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
    (8, b'aGVsbG8=')"###
    );
    register!(B64DECODE, cr###"b64decode($module, /, s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)
--

Decode an ASCII string or bytes-like Base64 value.

By default this follows Python's lenient Base64 behavior. Strict alphabet
validation, unpadded input, a custom ignored-byte set, and canonical tail
bit validation are available for protocols with tighter requirements.

Args:
    s: ASCII text or bytes-like Base64 data.
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
    b'hello'"###);
    register!(
        STANDARD_B64DECODE,
        cr###"standard_b64decode($module, /, s)
--

Decode padded Base64 using the standard alphabet.

Non-alphabet characters are discarded in the same lenient manner as
Python's base64.standard_b64decode function.

Args:
    s: ASCII text or bytes-like Base64 data.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The remaining Base64 data has invalid padding.
    TypeError: s has an unsupported type.
    ValueError: Text input contains non-ASCII characters.

Examples:
    >>> standard_b64decode(b'aGVsbG8=')
    b'hello'"###
    );
    register!(
        STANDARD_B64DECODE_INTO,
        cr###"standard_b64decode_into($module, /, s, output)
--

Decode standard Base64 into a reusable bytearray.

Args:
    s: ASCII text or bytes-like Base64 data.
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
    b'hello'"###
    );
    register!(
        B64DECODE_BATCH,
        cr###"b64decode_batch($module, /, items, altchars=None, validate=False)
--

Decode a list of padded Base64 values.

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
    [b'one', b'two']"###
    );
    register!(
        B64DECODE_BATCH_INTO,
        cr###"b64decode_batch_into($module, /, items, outputs, altchars=None, validate=False)
--

Decode each padded Base64 item into a matching reusable bytearray.

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
    [b'one', b'two']"###
    );
    register!(B64DECODE_INTO, cr###"b64decode_into($module, /, s, output, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)
--

Decode Base64 data into a reusable bytearray.

The options match b64decode. The destination keeps its size and bytes after
the returned prefix remain unchanged. On malformed input, part of the
destination prefix may already have been modified.

Args:
    s: ASCII text or bytes-like Base64 data.
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
    (5, b'hello')"###);
    register!(
        URLSAFE_B64DECODE,
        cr###"urlsafe_b64decode($module, /, s, *, padded=True)
--

Decode Base64 using the URL-safe alphabet.

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
    b'\xfb\xff'"###
    );
    register!(
        URLSAFE_B64DECODE_INTO,
        cr###"urlsafe_b64decode_into($module, /, s, output, *, padded=True)
--

Decode URL-safe Base64 into a reusable bytearray.

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
    b'\xfb\xff'"###
    );
    register!(
        STANDARD_B64ENCODE_BATCH,
        cr###"standard_b64encode_batch($module, /, items)
--

Encode each item with the padded standard Base64 alphabet."###
    );
    register!(
        STANDARD_B64ENCODE_BATCH_INTO,
        cr###"standard_b64encode_batch_into($module, /, items, outputs)
--

Encode each item into its matching reusable bytearray."###
    );
    register!(
        URLSAFE_B64ENCODE_BATCH,
        cr###"urlsafe_b64encode_batch($module, /, items)
--

Encode each item with the padded URL-safe Base64 alphabet."###
    );
    register!(
        URLSAFE_B64ENCODE_BATCH_INTO,
        cr###"urlsafe_b64encode_batch_into($module, /, items, outputs)
--

Encode each item with the URL-safe alphabet into its matching reusable bytearray."###
    );
    register!(
        STANDARD_B64DECODE_BATCH,
        cr###"standard_b64decode_batch($module, /, items)
--

Decode each item with the padded standard Base64 alphabet."###
    );
    register!(
        STANDARD_B64DECODE_BATCH_INTO,
        cr###"standard_b64decode_batch_into($module, /, items, outputs)
--

Decode each item into its matching reusable bytearray."###
    );
    register!(
        URLSAFE_B64DECODE_BATCH,
        cr###"urlsafe_b64decode_batch($module, /, items)
--

Decode each item with the padded URL-safe Base64 alphabet."###
    );
    register!(
        URLSAFE_B64DECODE_BATCH_INTO,
        cr###"urlsafe_b64decode_batch_into($module, /, items, outputs)
--

Decode each URL-safe item into its matching reusable bytearray."###
    );
    assert_eq!(
        method_count, BINDING_COUNT,
        "Base64 method table must match its schema"
    );
}

static METHODS_INIT: Once = Once::new();

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    let version_info = module.py().version_info();
    let version = (version_info.major, version_info.minor);
    METHODS_INIT.call_once(|| unsafe { initialize_methods(methods, version) });
    unsafe { add_methods(module, methods) }
}
