//! CPython registration and callbacks, routed to encode, decode, or batch.

use super::{batch, decode, encode};
use crate::bindings::runtime::{add_methods, return_function_result};
use crate::bindings::schema::base64::{BINDING_COUNT, register_all};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyInt, PyList, PyModule};
use pyo3::{PyTypeInfo, ffi};
use std::sync::Once;

static mut METHODS: [ffi::PyMethodDef; BINDING_COUNT + 1] =
    [const { ffi::PyMethodDef::zeroed() }; BINDING_COUNT + 1];

static METHODS_INIT: Once = Once::new();

pub(crate) unsafe fn add_to_module(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let methods = std::ptr::addr_of_mut!(METHODS).cast::<ffi::PyMethodDef>();
    let version_info = module.py().version_info();
    let version = (version_info.major, version_info.minor);
    METHODS_INIT.call_once(|| unsafe { register_all(methods, version) });
    unsafe { add_methods(module, methods) }
}

fn return_bound<T: PyTypeInfo>(
    py: Python<'_>,
    result: PyResult<Bound<'_, T>>,
) -> *mut ffi::PyObject {
    unsafe { return_function_result(py, result.map(Bound::into_ptr)) }
}

fn return_usize(py: Python<'_>, result: PyResult<usize>) -> *mut ffi::PyObject {
    unsafe { return_function_result(py, result.map(|value| PyInt::new(py, value).into_ptr())) }
}

macro_rules! callback {
    ($name:ident, |$py:ident; $($parameter:ident),+| $body:block) => {
        pub(in crate::bindings) unsafe extern "C" fn $name(
            _self: *mut ffi::PyObject,
            args: *const *mut ffi::PyObject,
            nargs: isize,
            keywords: *mut ffi::PyObject,
        ) -> *mut ffi::PyObject {
            unsafe { crate::bindings::schema::base64::$name(args, nargs, keywords, |$py, $($parameter),+| $body) }
        }
    };
}

callback! {
    standard_b64encode, |py; s| {
        return_bound(py, encode::standard_b64encode(py, s.raw(py)))
    }
}

callback! {
    standard_b64encode_into, |py; s, output| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            encode::standard_b64encode_into(s.raw(py), output)
        })();
        return_usize(py, result)
    }
}

callback! {
    urlsafe_b64encode, |py; s, padded| {
        let result = padded
            .truthy(py)
            .and_then(|padded| encode::urlsafe_b64encode(py, s.raw(py), padded));
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_into, |py; s, output, padded| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            encode::urlsafe_b64encode_into(s.raw(py), output, padded.truthy(py)?)
        })();
        return_usize(py, result)
    }
}

callback! {
    b64encode, |py; s, altchars, padded, wrapcol| {
        let result = (|| {
            encode::b64encode(
                py,
                s.raw(py),
                altchars.optional(py),
                padded.truthy(py)?,
                wrapcol.extract_i128(py)?,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_batch, |py; items, altchars| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64encode_batch(py, items, altchars.optional(py))
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_batch_into, |py; items, outputs, altchars| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64encode_batch_into(py, items, outputs, altchars.optional(py))
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64encode_batch, |py; items| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64encode_batch_parsed(py, items, None)
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64encode_batch_into, |py; items, outputs| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64encode_batch_into_parsed(py, items, outputs, None)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_batch, |py; items| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64encode_batch_parsed(py, items, Some(*b"-_"))
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_batch_into, |py; items, outputs| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64encode_batch_into_parsed(py, items, outputs, Some(*b"-_"))
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_into, |py; s, output, altchars, padded, wrapcol| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            encode::b64encode_into(
                py,
                s.raw(py),
                output,
                altchars.optional(py),
                padded.truthy(py)?,
                wrapcol.extract_i128(py)?,
            )
        })();
        return_usize(py, result)
    }
}

callback! {
    b64decode, |py; s, altchars, validate, padded, ignorechars, canonical| {
        let result = (|| {
            decode::b64decode(
                py,
                s.raw(py),
                altchars.optional(py),
                validate.optional_truthy(py)?,
                padded.truthy(py)?,
                ignorechars.provided(py),
                canonical.truthy(py)?,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64decode, |py; s| {
        return_bound(py, decode::standard_b64decode(py, s.raw(py)))
    }
}

callback! {
    standard_b64decode_into, |py; s, output| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            decode::standard_b64decode_into(py, s.raw(py), output)
        })();
        return_usize(py, result)
    }
}

callback! {
    standard_b64decode_batch, |py; items| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64decode_batch_parsed(py, items, None, false)
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64decode_batch_into, |py; items, outputs| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64decode_batch_into_parsed(py, items, outputs, None, false)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_batch, |py; items| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64decode_batch_parsed(py, items, Some(*b"-_"), false)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_batch_into, |py; items, outputs| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64decode_batch_into_parsed(py, items, outputs, Some(*b"-_"), false)
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_batch, |py; items, altchars, validate| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            batch::b64decode_batch(py, items, altchars.optional(py), validate.truthy(py)?)
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_batch_into, |py; items, outputs, altchars, validate| {
        let result = (|| {
            let items = items.raw(py).cast::<PyList>()?;
            let outputs = outputs.raw(py).cast::<PyList>()?;
            batch::b64decode_batch_into(
                py,
                items,
                outputs,
                altchars.optional(py),
                validate.truthy(py)?,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_into, |py; s, output, altchars, validate, padded, ignorechars, canonical| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            decode::b64decode_into(
                py,
                s.raw(py),
                output,
                altchars.optional(py),
                validate.optional_truthy(py)?,
                padded.truthy(py)?,
                ignorechars.provided(py),
                canonical.truthy(py)?,
            )
        })();
        return_usize(py, result)
    }
}

callback! {
    urlsafe_b64decode, |py; s, padded| {
        let result = padded
            .truthy(py)
            .and_then(|padded| decode::urlsafe_b64decode(py, s.raw(py), padded));
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_into, |py; s, output, padded| {
        let result = (|| {
            let output = output.raw(py).cast::<PyByteArray>()?;
            decode::urlsafe_b64decode_into(py, s.raw(py), output, padded.truthy(py)?)
        })();
        return_usize(py, result)
    }
}
