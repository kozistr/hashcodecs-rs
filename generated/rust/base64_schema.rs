// tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.

pub(super) const BINDING_COUNT: usize = 24;

binding! {
    B64ENCODE: 4 {
        name: c"b64encode",
        callback: b64encode,
        parameters: [c"s", c"altchars", c"padded", c"wrapcol"],
        max_positional: 2,
        required: 1,
        documentation: cr#"b64encode($module, /, s, altchars=None, *, padded=True, wrapcol=0)
--

Encode a bytes-like object as Base64.

The function uses the standard RFC 4648 alphabet by default. ``altchars``
can replace the "+" and "/" characters. Use ``padded`` and ``wrapcol``
to select the wire format.

Args:
    s: Contiguous bytes-like data to encode.
    altchars: A two-byte replacement for "+" and "/", or None for the
        standard alphabet.
    padded: Append trailing "=" padding when required.
    wrapcol: Maximum encoded characters per line. Zero disables wrapping.

Returns:
    New Base64-encoded bytes.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: altchars is not exactly two bytes or wrapcol is negative.

Examples:
    >>> b64encode(b'hello')
    b'aGVsbG8='
    >>> b64encode(b'hello', padded=False, wrapcol=4)
    b'aGVs\nbG8'"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64encode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64ENCODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::None),
                Argument::new(values[2], DefaultValue::Bool(true)),
                Argument::new(values[3], DefaultValue::I128(0)),
            )
        })
    }
}

binding! {
    B64ENCODE_BATCH: 2 {
        name: c"b64encode_batch",
        callback: b64encode_batch,
        parameters: [c"items", c"altchars"],
        max_positional: 2,
        required: 1,
        documentation: cr#"b64encode_batch($module, /, items, altchars=None)
--

Encode a list of bytes-like objects as padded Base64.

Every item uses the same alphabet. The result order matches the input
order. The function stops at the first invalid item and discards the
partial result list.

Args:
    items: A list of contiguous bytes-like objects to encode.
    altchars: A two-byte replacement for "+" and "/", or None for the
        standard alphabet.

Returns:
    One new Base64 byte string for each input item.

Raises:
    TypeError: items is not a list or an item is not bytes-like.
    ValueError: altchars is not exactly two bytes.

Examples:
    >>> b64encode_batch([b'one', b'two'])
    [b'b25l', b'dHdv']"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64encode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64ENCODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::None),
            )
        })
    }
}

binding! {
    B64ENCODE_BATCH_INTO: 3 {
        name: c"b64encode_batch_into",
        callback: b64encode_batch_into,
        parameters: [c"items", c"outputs", c"altchars"],
        max_positional: 3,
        required: 2,
        documentation: cr#"b64encode_batch_into($module, /, items, outputs, altchars=None)
--

Encode each item into a matching reusable bytearray.

The two lists must have equal length. Each destination must be a different
bytearray. The function stops at the first error. If an item fails, the
function does not restore destinations that it changed.

Args:
    items: A list of contiguous bytes-like objects to encode.
    outputs: An equal-length list of distinct destination bytearrays.
    altchars: A two-byte replacement for "+" and "/", or None for the
        standard alphabet.

Returns:
    The number of bytes that the function writes to each destination, in
    input order.

Raises:
    TypeError: A container, input item, or destination has an invalid type.
    ValueError: The list lengths differ, two entries use the same
        destination, a destination is too small, or altchars is not exactly
        two bytes.

Examples:
    >>> outputs = [bytearray(4), bytearray(4)]
    >>> b64encode_batch_into([b'one', b'two'], outputs)
    [4, 4]
    >>> [bytes(output) for output in outputs]
    [b'b25l', b'dHdv']"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64encode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64ENCODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::None),
            )
        })
    }
}

binding! {
    B64ENCODE_INTO: 5 {
        name: c"b64encode_into",
        callback: b64encode_into,
        parameters: [c"s", c"output", c"altchars", c"padded", c"wrapcol"],
        max_positional: 3,
        required: 2,
        documentation: cr#"b64encode_into($module, /, s, output, altchars=None, *, padded=True, wrapcol=0)
--

Encode a bytes-like object as Base64 into a reusable bytearray.

The destination keeps its size. The function changes only the prefix that
the return value identifies. It does not change bytes after that prefix.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the complete result.
    altchars: A two-byte replacement for "+" and "/", or None for the
        standard alphabet.
    padded: Append trailing "=" padding when required.
    wrapcol: Maximum encoded characters per line. Zero disables wrapping.

Returns:
    The number of bytes that the function writes to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: The destination is too small, altchars is not exactly two
        bytes, or wrapcol is negative.

Examples:
    >>> output = bytearray(12)
    >>> written = b64encode_into(b'hello', output)
    >>> written, bytes(output[:written])
    (8, b'aGVsbG8=')"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64encode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64ENCODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::None),
                Argument::new(values[3], DefaultValue::Bool(true)),
                Argument::new(values[4], DefaultValue::I128(0)),
            )
        })
    }
}

binding! {
    B64DECODE: 6 {
        name: c"b64decode",
        callback: b64decode,
        parameters: [c"s", c"altchars", c"validate", c"padded", c"ignorechars", c"canonical"],
        max_positional: 3,
        required: 1,
        documentation: cr#"b64decode($module, /, s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)
--

Decode an ASCII string or bytes-like Base64 value.

By default, the function uses Python's lenient Base64 behavior. The options
can require strict alphabet checks, canonical tail bits, or padded input.
The options can also select the ignored bytes.

Args:
    s: ASCII text or bytes-like Base64 data.
    altchars: Two characters replacing "+" and "/", or None for the
        standard alphabet.
    validate: If true, reject bytes outside the alphabet. The function uses
        lenient mode by default unless the caller supplies ignorechars.
    padded: If true, require normal padding and complete four-byte groups.
        If false, accept a final group without padding.
    ignorechars: Bytes permitted outside the alphabet in lenient mode.
    canonical: Reject non-zero unused bits in the final Base64 quantum.

Returns:
    New decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data, padding, or tail bits.
    TypeError: An argument has an unsupported type.
    ValueError: Text input is not ASCII or altchars is not length two.

Examples:
    >>> b64decode(b'aGVsbG8=', validate=True)
    b'hello'
    >>> b64decode(b'aGVsbG8', padded=False, canonical=True)
    b'hello'"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64decode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64DECODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::None),
                Argument::new(values[2], DefaultValue::Missing),
                Argument::new(values[3], DefaultValue::Bool(true)),
                Argument::new(values[4], DefaultValue::Missing),
                Argument::new(values[5], DefaultValue::Bool(false)),
            )
        })
    }
}

binding! {
    B64DECODE_BATCH: 3 {
        name: c"b64decode_batch",
        callback: b64decode_batch,
        parameters: [c"items", c"altchars", c"validate"],
        max_positional: 3,
        required: 1,
        documentation: cr#"b64decode_batch($module, /, items, altchars=None, validate=False)
--

Decode a list of padded Base64 values.

Every item uses the same alphabet and validation mode. The result order
matches the input order. An invalid item stops the function. The function
does not return a partial list.

Args:
    items: A list of ASCII strings or bytes-like Base64 values.
    altchars: Two characters replacing "+" and "/", or None for the
        standard alphabet.
    validate: Reject bytes outside the selected alphabet when true.

Returns:
    One new decoded byte string for each input item.

Raises:
    binascii.Error: An item contains invalid Base64 data or padding.
    TypeError: items is not a list or an item has an invalid type.
    ValueError: Text is not ASCII or altchars is not length two.

Examples:
    >>> b64decode_batch([b'b25l', b'dHdv'], validate=True)
    [b'one', b'two']"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64decode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64DECODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::None),
                Argument::new(values[2], DefaultValue::Bool(false)),
            )
        })
    }
}

binding! {
    B64DECODE_BATCH_INTO: 4 {
        name: c"b64decode_batch_into",
        callback: b64decode_batch_into,
        parameters: [c"items", c"outputs", c"altchars", c"validate"],
        max_positional: 4,
        required: 2,
        documentation: cr#"b64decode_batch_into($module, /, items, outputs, altchars=None, validate=False)
--

Decode each padded Base64 item into a matching reusable bytearray.

Each destination keeps its size. The function changes only written
prefixes. The function stops at the first error. It does not restore prior
destinations. It can change part of the failing destination.

Args:
    items: A list of ASCII strings or bytes-like Base64 values.
    outputs: An equal-length list of distinct destination bytearrays.
    altchars: Two characters replacing "+" and "/", or None for the
        standard alphabet.
    validate: Reject bytes outside the selected alphabet when true.

Returns:
    The number of decoded bytes that the function writes to each
    destination.

Raises:
    binascii.Error: An item contains invalid Base64 data or padding.
    TypeError: A container, item, or destination has an invalid type.
    ValueError: The lists differ in length or two entries use the same
        destination. The function also raises this error if a destination
        is too small, text is not ASCII, or altchars is not length two.

Examples:
    >>> outputs = [bytearray(3), bytearray(3)]
    >>> b64decode_batch_into([b'b25l', b'dHdv'], outputs, validate=True)
    [3, 3]
    >>> [bytes(output) for output in outputs]
    [b'one', b'two']"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64decode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64DECODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::None),
                Argument::new(values[3], DefaultValue::Bool(false)),
            )
        })
    }
}

binding! {
    B64DECODE_INTO: 7 {
        name: c"b64decode_into",
        callback: b64decode_into,
        parameters: [c"s", c"output", c"altchars", c"validate", c"padded", c"ignorechars", c"canonical"],
        max_positional: 4,
        required: 2,
        documentation: cr#"b64decode_into($module, /, s, output, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)
--

Decode Base64 data into a reusable bytearray.

The options match b64decode. The destination keeps its size. The function
does not change bytes after the returned prefix. Malformed input can change
part of the destination prefix.

All native modes decode into the destination. This includes custom
ignore-character and canonical modes. These modes do not allocate a
temporary decoded buffer.

Args:
    s: ASCII text or bytes-like Base64 data.
    output: Destination bytearray with room for the complete result.
    altchars: Two characters replacing "+" and "/", or None for the
        standard alphabet.
    validate: If true, reject bytes outside the alphabet. The function uses
        lenient mode by default unless the caller supplies ignorechars.
    padded: If true, require padding and complete four-byte groups. If
        false, accept a final group without padding.
    ignorechars: Bytes permitted outside the alphabet in lenient mode.
    canonical: Reject non-zero unused bits in the final Base64 quantum.

Returns:
    The number of decoded bytes that the function writes to output.

Raises:
    binascii.Error: The input has invalid Base64 data, padding, or tail bits.
    TypeError: An argument has an unsupported type.
    ValueError: The destination is too small, text is not ASCII, or
        altchars is not length two.

Examples:
    >>> output = bytearray(8)
    >>> written = b64decode_into(b'aGVsbG8=', output, validate=True)
    >>> written, bytes(output[:written])
    (5, b'hello')"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn b64decode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument, Argument, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        B64DECODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::None),
                Argument::new(values[3], DefaultValue::Missing),
                Argument::new(values[4], DefaultValue::Bool(true)),
                Argument::new(values[5], DefaultValue::Missing),
                Argument::new(values[6], DefaultValue::Bool(false)),
            )
        })
    }
}

binding! {
    STANDARD_B64ENCODE: 1 {
        name: c"standard_b64encode",
        callback: standard_b64encode,
        parameters: [c"s"],
        max_positional: 1,
        required: 1,
        documentation: cr#"standard_b64encode($module, /, s)
--

Encode bytes with the padded standard Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.

Returns:
    New Base64 bytes that use "+" and "/".

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> standard_b64encode(b'hello')
    b'aGVsbG8='"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64encode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64ENCODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64ENCODE_INTO: 2 {
        name: c"standard_b64encode_into",
        callback: standard_b64encode_into,
        parameters: [c"s", c"output"],
        max_positional: 2,
        required: 2,
        documentation: cr"standard_b64encode_into($module, /, s, output)
--

Encode bytes with the standard alphabet into a reusable bytearray.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the padded result.

Returns:
    The number of bytes that the function writes to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: output is too small.

Examples:
    >>> output = bytearray(8)
    >>> standard_b64encode_into(b'hello', output)
    8
    >>> bytes(output)
    b'aGVsbG8='",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64encode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64ENCODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64ENCODE_BATCH: 1 {
        name: c"standard_b64encode_batch",
        callback: standard_b64encode_batch,
        parameters: [c"items"],
        max_positional: 1,
        required: 1,
        documentation: cr"standard_b64encode_batch($module, /, items)
--

Encode each item with the padded standard Base64 alphabet.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64encode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64ENCODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64ENCODE_BATCH_INTO: 2 {
        name: c"standard_b64encode_batch_into",
        callback: standard_b64encode_batch_into,
        parameters: [c"items", c"outputs"],
        max_positional: 2,
        required: 2,
        documentation: cr"standard_b64encode_batch_into($module, /, items, outputs)
--

Encode each item into its matching reusable bytearray.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64encode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64ENCODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64DECODE_BATCH: 1 {
        name: c"standard_b64decode_batch",
        callback: standard_b64decode_batch,
        parameters: [c"items"],
        max_positional: 1,
        required: 1,
        documentation: cr"standard_b64decode_batch($module, /, items)
--

Decode each item with the padded standard Base64 alphabet.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64decode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64DECODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64DECODE_BATCH_INTO: 2 {
        name: c"standard_b64decode_batch_into",
        callback: standard_b64decode_batch_into,
        parameters: [c"items", c"outputs"],
        max_positional: 2,
        required: 2,
        documentation: cr"standard_b64decode_batch_into($module, /, items, outputs)
--

Decode each item into its matching reusable bytearray.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64decode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64DECODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64DECODE: 1 {
        name: c"standard_b64decode",
        callback: standard_b64decode,
        parameters: [c"s"],
        max_positional: 1,
        required: 1,
        documentation: cr"standard_b64decode($module, /, s)
--

Decode padded Base64 using the standard alphabet.

The function discards bytes outside the alphabet. This behavior matches
Python's base64.standard_b64decode function.

Args:
    s: ASCII text or bytes-like Base64 data.

Returns:
    New decoded bytes.

Raises:
    binascii.Error: The remaining Base64 data has invalid padding.
    TypeError: s has an unsupported type.
    ValueError: Text input contains non-ASCII characters.

Examples:
    >>> standard_b64decode(b'aGVsbG8=')
    b'hello'",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64decode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64DECODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    STANDARD_B64DECODE_INTO: 2 {
        name: c"standard_b64decode_into",
        callback: standard_b64decode_into,
        parameters: [c"s", c"output"],
        max_positional: 2,
        required: 2,
        documentation: cr"standard_b64decode_into($module, /, s, output)
--

Decode standard Base64 into a reusable bytearray.

Args:
    s: ASCII text or bytes-like Base64 data.
    output: Destination bytearray with room for the decoded result.

Returns:
    The number of decoded bytes that the function writes to output.

Raises:
    binascii.Error: The input has invalid Base64 padding.
    TypeError: An argument has an unsupported type.
    ValueError: The destination is too small or text is not ASCII.

Examples:
    >>> output = bytearray(5)
    >>> standard_b64decode_into(b'aGVsbG8=', output)
    5
    >>> bytes(output)
    b'hello'",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn standard_b64decode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        STANDARD_B64DECODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    URLSAFE_B64ENCODE_BATCH: 1 {
        name: c"urlsafe_b64encode_batch",
        callback: urlsafe_b64encode_batch,
        parameters: [c"items"],
        max_positional: 1,
        required: 1,
        documentation: cr"urlsafe_b64encode_batch($module, /, items)
--

Encode each item with the padded URL-safe Base64 alphabet.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64encode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64ENCODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    URLSAFE_B64ENCODE_BATCH_INTO: 2 {
        name: c"urlsafe_b64encode_batch_into",
        callback: urlsafe_b64encode_batch_into,
        parameters: [c"items", c"outputs"],
        max_positional: 2,
        required: 2,
        documentation: cr"urlsafe_b64encode_batch_into($module, /, items, outputs)
--

Encode each item with the URL-safe alphabet into its matching reusable bytearray.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64encode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64ENCODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    URLSAFE_B64DECODE_BATCH: 1 {
        name: c"urlsafe_b64decode_batch",
        callback: urlsafe_b64decode_batch,
        parameters: [c"items"],
        max_positional: 1,
        required: 1,
        documentation: cr"urlsafe_b64decode_batch($module, /, items)
--

Decode each item with the padded URL-safe Base64 alphabet.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64decode_batch(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64DECODE_BATCH.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
            )
        })
    }
}

binding! {
    URLSAFE_B64DECODE_BATCH_INTO: 2 {
        name: c"urlsafe_b64decode_batch_into",
        callback: urlsafe_b64decode_batch_into,
        parameters: [c"items", c"outputs"],
        max_positional: 2,
        required: 2,
        documentation: cr"urlsafe_b64decode_batch_into($module, /, items, outputs)
--

Decode each URL-safe item into its matching reusable bytearray.",
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64decode_batch_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64DECODE_BATCH_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
            )
        })
    }
}

binding! {
    URLSAFE_B64ENCODE: 2 {
        name: c"urlsafe_b64encode",
        callback: urlsafe_b64encode,
        parameters: [c"s", c"padded"],
        max_positional: 1,
        required: 1,
        documentation: cr#"urlsafe_b64encode($module, /, s, *, padded=True)
--

Encode bytes with the URL-safe Base64 alphabet.

Args:
    s: Contiguous bytes-like data to encode.
    padded: Append trailing "=" padding when required.

Returns:
    New Base64 bytes that use "-" and "_".

Raises:
    TypeError: s is not a contiguous bytes-like object.

Examples:
    >>> urlsafe_b64encode(bytes([251, 255]), padded=False)
    b'-_8'"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64encode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64ENCODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Bool(true)),
            )
        })
    }
}

binding! {
    URLSAFE_B64ENCODE_INTO: 3 {
        name: c"urlsafe_b64encode_into",
        callback: urlsafe_b64encode_into,
        parameters: [c"s", c"output", c"padded"],
        max_positional: 2,
        required: 2,
        documentation: cr#"urlsafe_b64encode_into($module, /, s, output, *, padded=True)
--

Encode bytes with the URL-safe alphabet into a reusable bytearray.

Args:
    s: Contiguous bytes-like data to encode.
    output: Destination bytearray with room for the complete result.
    padded: Append trailing "=" padding when required.

Returns:
    The number of bytes that the function writes to output.

Raises:
    TypeError: An argument has an unsupported type.
    ValueError: output is too small.

Examples:
    >>> output = bytearray(4)
    >>> urlsafe_b64encode_into(bytes([251, 255]), output)
    4
    >>> bytes(output)
    b'-_8='"#,
        python_315_documentation: None,
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64encode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64ENCODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::Bool(true)),
            )
        })
    }
}

binding! {
    URLSAFE_B64DECODE: 2 {
        name: c"urlsafe_b64decode",
        callback: urlsafe_b64decode,
        parameters: [c"s", c"padded"],
        max_positional: 1,
        required: 1,
        documentation: cr"urlsafe_b64decode($module, /, s, *, padded=True)
--

Decode Base64 using the URL-safe alphabet.

CPython uses true as the padded default through Python 3.14. CPython uses
false from Python 3.15.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    padded: If true, require padding. If false, accept a final group without
        padding.

Returns:
    New decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: s has an unsupported type.
    ValueError: Text input contains non-ASCII characters.

Examples:
    >>> urlsafe_b64decode(b'-_8=', padded=True)
    b'\xfb\xff'",
        python_315_documentation: Some(cr"urlsafe_b64decode($module, /, s, *, padded=False)
--

Decode Base64 using the URL-safe alphabet.

CPython uses true as the padded default through Python 3.14. CPython uses
false from Python 3.15.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    padded: If true, require padding. If false, accept a final group without
        padding.

Returns:
    New decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: s has an unsupported type.
    ValueError: Text input contains non-ASCII characters.

Examples:
    >>> urlsafe_b64decode(b'-_8=', padded=True)
    b'\xfb\xff'"),
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64decode(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64DECODE.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::VersionedBool { before: true, since: (3, 15), after: false }),
            )
        })
    }
}

binding! {
    URLSAFE_B64DECODE_INTO: 3 {
        name: c"urlsafe_b64decode_into",
        callback: urlsafe_b64decode_into,
        parameters: [c"s", c"output", c"padded"],
        max_positional: 2,
        required: 2,
        documentation: cr"urlsafe_b64decode_into($module, /, s, output, *, padded=True)
--

Decode URL-safe Base64 into a reusable bytearray.

CPython uses true as the padded default through Python 3.14. CPython uses
false from Python 3.15.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    output: Destination bytearray with room for the decoded result.
    padded: If true, require padding. If false, accept a final group without
        padding.

Returns:
    The number of decoded bytes that the function writes to output.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: An argument has an unsupported type.
    ValueError: The destination is too small or text is not ASCII.

Examples:
    >>> output = bytearray(2)
    >>> urlsafe_b64decode_into(b'-_8=', output, padded=True)
    2
    >>> bytes(output)
    b'\xfb\xff'",
        python_315_documentation: Some(cr"urlsafe_b64decode_into($module, /, s, output, *, padded=False)
--

Decode URL-safe Base64 into a reusable bytearray.

CPython uses true as the padded default through Python 3.14. CPython uses
false from Python 3.15.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    output: Destination bytearray with room for the decoded result.
    padded: If true, require padding. If false, accept a final group without
        padding.

Returns:
    The number of decoded bytes that the function writes to output.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: An argument has an unsupported type.
    ValueError: The destination is too small or text is not ASCII.

Examples:
    >>> output = bytearray(2)
    >>> urlsafe_b64decode_into(b'-_8=', output, padded=True)
    2
    >>> bytes(output)
    b'\xfb\xff'"),
    }
}

#[inline(always)]
pub(super) unsafe fn urlsafe_b64decode_into(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    operation: impl FnOnce(Python<'_>, Argument, Argument, Argument) -> *mut ffi::PyObject,
) -> *mut ffi::PyObject {
    unsafe {
        URLSAFE_B64DECODE_INTO.invoke(args, nargs, keywords, |py, values| {
            operation(
                py,
                Argument::new(values[0], DefaultValue::Required),
                Argument::new(values[1], DefaultValue::Required),
                Argument::new(values[2], DefaultValue::VersionedBool { before: true, since: (3, 15), after: false }),
            )
        })
    }
}

pub(super) unsafe fn register_all(methods: *mut ffi::PyMethodDef, version: (u8, u8)) {
    let mut method_count = 0;
    unsafe { B64ENCODE.register(methods, &mut method_count, version) };
    unsafe { B64ENCODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { B64ENCODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { B64ENCODE_INTO.register(methods, &mut method_count, version) };
    unsafe { B64DECODE.register(methods, &mut method_count, version) };
    unsafe { B64DECODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { B64DECODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { B64DECODE_INTO.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64ENCODE.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64ENCODE_INTO.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64ENCODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64ENCODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64DECODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64DECODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64DECODE.register(methods, &mut method_count, version) };
    unsafe { STANDARD_B64DECODE_INTO.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64ENCODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64ENCODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64DECODE_BATCH.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64DECODE_BATCH_INTO.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64ENCODE.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64ENCODE_INTO.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64DECODE.register(methods, &mut method_count, version) };
    unsafe { URLSAFE_B64DECODE_INTO.register(methods, &mut method_count, version) };
    assert_eq!(
        method_count, BINDING_COUNT,
        "Base64 method table must match its generated schema",
    );
}
