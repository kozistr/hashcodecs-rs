use core::ptr;
use std::ffi::CStr;

use pyo3::ffi;
use pyo3::prelude::*;

use crate::bindings::arguments::parse_raw_arguments;
use crate::bindings::runtime::catch_unwind_callback;

pub(super) type Callback = unsafe extern "C" fn(
    *mut ffi::PyObject,
    *const *mut ffi::PyObject,
    isize,
    *mut ffi::PyObject,
) -> *mut ffi::PyObject;

#[derive(Clone, Copy)]
pub(super) struct Availability {
    since: (u8, u8),
}

impl Availability {
    const SUPPORTED: Self = Self { since: (3, 10) };

    pub(super) const fn includes(self, version: (u8, u8)) -> bool {
        version.0 > self.since.0 || (version.0 == self.since.0 && version.1 >= self.since.1)
    }
}

#[derive(Clone, Copy)]
struct Signatures {
    default: &'static CStr,
    python_315: Option<&'static CStr>,
}

impl Signatures {
    fn select(self, version: (u8, u8)) -> &'static CStr {
        if version >= (3, 15) {
            self.python_315.unwrap_or(self.default)
        } else {
            self.default
        }
    }
}

#[derive(Clone, Copy)]
struct Parser<const N: usize> {
    parameters: [&'static CStr; N],
    max_positional: usize,
    required: usize,
}

#[derive(Clone, Copy)]
struct Documentation {
    signatures: Signatures,
    python_315: Option<&'static CStr>,
}

pub(super) struct Binding<const N: usize> {
    name: &'static CStr,
    callback: Callback,
    parser: Parser<N>,
    documentation: Documentation,
    availability: Availability,
}

impl<const N: usize> Binding<N> {
    pub(super) unsafe fn invoke(
        &self,
        args: *const *mut ffi::PyObject,
        nargs: isize,
        keywords: *mut ffi::PyObject,
        operation: impl FnOnce(Python<'_>, [*mut ffi::PyObject; N]) -> *mut ffi::PyObject,
    ) -> *mut ffi::PyObject {
        let py = unsafe { Python::assume_attached() };
        catch_unwind_callback(py, || unsafe {
            let Some(values) = parse_raw_arguments(
                args,
                nargs,
                keywords,
                self.name.as_ptr(),
                self.parser.parameters.map(CStr::as_ptr),
                self.parser.max_positional,
                self.parser.required,
            ) else {
                return ptr::null_mut();
            };
            operation(py, values)
        })
    }

    pub(super) fn is_available(&self, version: (u8, u8)) -> bool {
        self.availability.includes(version)
    }

    fn validate_documentation(&self, version: (u8, u8), documentation: &'static CStr) {
        assert_eq!(
            documentation.to_bytes().split(|byte| *byte == b'\n').next(),
            Some(self.documentation.signatures.select(version).to_bytes()),
            "Base64 binding documentation must start with its declared text signature",
        );
    }

    pub(super) unsafe fn register(
        &self,
        methods: *mut ffi::PyMethodDef,
        method_count: &mut usize,
        version: (u8, u8),
        default_documentation: &'static CStr,
    ) {
        assert!(self.is_available(version));
        let documentation = if version >= (3, 15) {
            self.documentation
                .python_315
                .unwrap_or(default_documentation)
        } else {
            default_documentation
        };
        self.validate_documentation(version, documentation);
        let method = ffi::PyMethodDef {
            ml_name: self.name.as_ptr(),
            ml_meth: ffi::PyMethodDefPointer {
                PyCFunctionFastWithKeywords: self.callback,
            },
            ml_flags: super::METHOD_FLAGS,
            ml_doc: documentation.as_ptr(),
        };
        unsafe { methods.add(*method_count).write(method) };
        *method_count += 1;
    }
}

macro_rules! binding {
    (
        $constant:ident: $count:literal {
            name: $name:expr,
            callback: $callback:ident,
            parameters: [$($parameter:expr),* $(,)?],
            max_positional: $max_positional:expr,
            required: $required:expr,
            signature: $signature:expr,
            python_315_signature: $python_315_signature:expr,
            python_315_documentation: $python_315_documentation:expr $(,)?
        }
    ) => {
        pub(super) const $constant: Binding<$count> = Binding {
            name: $name,
            callback: super::callbacks::$callback,
            parser: Parser {
                parameters: [$($parameter),*],
                max_positional: $max_positional,
                required: $required,
            },
            documentation: Documentation {
                signatures: Signatures {
                    default: $signature,
                    python_315: $python_315_signature,
                },
                python_315: $python_315_documentation,
            },
            availability: Availability::SUPPORTED,
        };
    };
}

binding! {
    STANDARD_B64ENCODE: 1 {
        name: c"standard_b64encode",
        callback: standard_b64encode,
        parameters: [c"s"],
        max_positional: 1,
        required: 1,
        signature: c"standard_b64encode($module, /, s)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    STANDARD_B64ENCODE_INTO: 2 {
        name: c"standard_b64encode_into",
        callback: standard_b64encode_into,
        parameters: [c"s", c"output"],
        max_positional: 2,
        required: 2,
        signature: c"standard_b64encode_into($module, /, s, output)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    URLSAFE_B64ENCODE: 2 {
        name: c"urlsafe_b64encode",
        callback: urlsafe_b64encode,
        parameters: [c"s", c"padded"],
        max_positional: 1,
        required: 1,
        signature: c"urlsafe_b64encode($module, /, s, *, padded=True)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    URLSAFE_B64ENCODE_INTO: 3 {
        name: c"urlsafe_b64encode_into",
        callback: urlsafe_b64encode_into,
        parameters: [c"s", c"output", c"padded"],
        max_positional: 2,
        required: 2,
        signature: c"urlsafe_b64encode_into($module, /, s, output, *, padded=True)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64ENCODE: 4 {
        name: c"b64encode",
        callback: b64encode,
        parameters: [c"s", c"altchars", c"padded", c"wrapcol"],
        max_positional: 2,
        required: 1,
        signature: c"b64encode($module, /, s, altchars=None, *, padded=True, wrapcol=0)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64ENCODE_BATCH: 2 {
        name: c"b64encode_batch",
        callback: b64encode_batch,
        parameters: [c"items", c"altchars"],
        max_positional: 2,
        required: 1,
        signature: c"b64encode_batch($module, /, items, altchars=None)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64ENCODE_BATCH_INTO: 3 {
        name: c"b64encode_batch_into",
        callback: b64encode_batch_into,
        parameters: [c"items", c"outputs", c"altchars"],
        max_positional: 3,
        required: 2,
        signature: c"b64encode_batch_into($module, /, items, outputs, altchars=None)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64ENCODE_INTO: 5 {
        name: c"b64encode_into",
        callback: b64encode_into,
        parameters: [c"s", c"output", c"altchars", c"padded", c"wrapcol"],
        max_positional: 3,
        required: 2,
        signature: c"b64encode_into($module, /, s, output, altchars=None, *, padded=True, wrapcol=0)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64DECODE: 6 {
        name: c"b64decode",
        callback: b64decode,
        parameters: [
            c"s",
            c"altchars",
            c"validate",
            c"padded",
            c"ignorechars",
            c"canonical",
        ],
        max_positional: 3,
        required: 1,
        signature: c"b64decode($module, /, s, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    STANDARD_B64DECODE: 1 {
        name: c"standard_b64decode",
        callback: standard_b64decode,
        parameters: [c"s"],
        max_positional: 1,
        required: 1,
        signature: c"standard_b64decode($module, /, s)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    STANDARD_B64DECODE_INTO: 2 {
        name: c"standard_b64decode_into",
        callback: standard_b64decode_into,
        parameters: [c"s", c"output"],
        max_positional: 2,
        required: 2,
        signature: c"standard_b64decode_into($module, /, s, output)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64DECODE_BATCH: 3 {
        name: c"b64decode_batch",
        callback: b64decode_batch,
        parameters: [c"items", c"altchars", c"validate"],
        max_positional: 3,
        required: 1,
        signature: c"b64decode_batch($module, /, items, altchars=None, validate=False)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64DECODE_BATCH_INTO: 4 {
        name: c"b64decode_batch_into",
        callback: b64decode_batch_into,
        parameters: [c"items", c"outputs", c"altchars", c"validate"],
        max_positional: 4,
        required: 2,
        signature: c"b64decode_batch_into($module, /, items, outputs, altchars=None, validate=False)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    B64DECODE_INTO: 7 {
        name: c"b64decode_into",
        callback: b64decode_into,
        parameters: [
            c"s",
            c"output",
            c"altchars",
            c"validate",
            c"padded",
            c"ignorechars",
            c"canonical",
        ],
        max_positional: 4,
        required: 2,
        signature: c"b64decode_into($module, /, s, output, altchars=None, validate=['NOT SPECIFIED'], *, padded=True, ignorechars=['NOT SPECIFIED'], canonical=False)",
        python_315_signature: None,
        python_315_documentation: None,
    }
}

binding! {
    URLSAFE_B64DECODE: 2 {
        name: c"urlsafe_b64decode",
        callback: urlsafe_b64decode,
        parameters: [c"s", c"padded"],
        max_positional: 1,
        required: 1,
        signature: c"urlsafe_b64decode($module, /, s, *, padded=True)",
        python_315_signature: Some(c"urlsafe_b64decode($module, /, s, *, padded=False)"),
        python_315_documentation: Some(cr"urlsafe_b64decode($module, /, s, *, padded=False)
--

Decode Base64 using the URL-safe alphabet.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    Newly allocated decoded bytes.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: s has an unsupported type.
    ValueError: Text input is not ASCII.

Examples:
    >>> urlsafe_b64decode(b'-_8')
    b'\xfb\xff'"),
    }
}

binding! {
    URLSAFE_B64DECODE_INTO: 3 {
        name: c"urlsafe_b64decode_into",
        callback: urlsafe_b64decode_into,
        parameters: [c"s", c"output", c"padded"],
        max_positional: 2,
        required: 2,
        signature: c"urlsafe_b64decode_into($module, /, s, output, *, padded=True)",
        python_315_signature: Some(c"urlsafe_b64decode_into($module, /, s, output, *, padded=False)"),
        python_315_documentation: Some(cr"urlsafe_b64decode_into($module, /, s, output, *, padded=False)
--

Decode URL-safe Base64 into a reusable bytearray.

Args:
    s: ASCII text or bytes-like URL-safe Base64 data.
    output: Destination bytearray with room for the result.
    padded: Require padding when true; accept an unpadded tail when false.

Returns:
    The number of decoded bytes written to output.

Raises:
    binascii.Error: The input has invalid Base64 data or padding.
    TypeError: An argument has an unsupported type.
    ValueError: output is too small or text is not ASCII.

Examples:
    >>> output = bytearray(2)
    >>> urlsafe_b64decode_into(b'-_8', output)
    2"),
    }
}

macro_rules! fixed_batch_binding {
    ($constant:ident, $callback:ident, $name:expr, $signature:expr) => {
        binding! {
            $constant: 1 {
                name: $name,
                callback: $callback,
                parameters: [c"items"],
                max_positional: 1,
                required: 1,
                signature: $signature,
                python_315_signature: None,
                python_315_documentation: None,
            }
        }
    };
    ($constant:ident, $callback:ident, $name:expr, $signature:expr, into) => {
        binding! {
            $constant: 2 {
                name: $name,
                callback: $callback,
                parameters: [c"items", c"outputs"],
                max_positional: 2,
                required: 2,
                signature: $signature,
                python_315_signature: None,
                python_315_documentation: None,
            }
        }
    };
}

fixed_batch_binding!(
    STANDARD_B64ENCODE_BATCH,
    standard_b64encode_batch,
    c"standard_b64encode_batch",
    c"standard_b64encode_batch($module, /, items)"
);
fixed_batch_binding!(
    STANDARD_B64ENCODE_BATCH_INTO,
    standard_b64encode_batch_into,
    c"standard_b64encode_batch_into",
    c"standard_b64encode_batch_into($module, /, items, outputs)",
    into
);
fixed_batch_binding!(
    URLSAFE_B64ENCODE_BATCH,
    urlsafe_b64encode_batch,
    c"urlsafe_b64encode_batch",
    c"urlsafe_b64encode_batch($module, /, items)"
);
fixed_batch_binding!(
    URLSAFE_B64ENCODE_BATCH_INTO,
    urlsafe_b64encode_batch_into,
    c"urlsafe_b64encode_batch_into",
    c"urlsafe_b64encode_batch_into($module, /, items, outputs)",
    into
);
fixed_batch_binding!(
    STANDARD_B64DECODE_BATCH,
    standard_b64decode_batch,
    c"standard_b64decode_batch",
    c"standard_b64decode_batch($module, /, items)"
);
fixed_batch_binding!(
    STANDARD_B64DECODE_BATCH_INTO,
    standard_b64decode_batch_into,
    c"standard_b64decode_batch_into",
    c"standard_b64decode_batch_into($module, /, items, outputs)",
    into
);
fixed_batch_binding!(
    URLSAFE_B64DECODE_BATCH,
    urlsafe_b64decode_batch,
    c"urlsafe_b64decode_batch",
    c"urlsafe_b64decode_batch($module, /, items)"
);
fixed_batch_binding!(
    URLSAFE_B64DECODE_BATCH_INTO,
    urlsafe_b64decode_batch_into,
    c"urlsafe_b64decode_batch_into",
    c"urlsafe_b64decode_batch_into($module, /, items, outputs)",
    into
);
