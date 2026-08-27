use pyo3::ffi;
use pyo3::types::{PyByteArray, PyList};

use super::*;

macro_rules! callback {
    ($name:ident, $binding:path, |$py:ident, $values:ident| $body:block) => {
        pub(super) unsafe extern "C" fn $name(
            _self: *mut ffi::PyObject,
            args: *const *mut ffi::PyObject,
            nargs: isize,
            keywords: *mut ffi::PyObject,
        ) -> *mut ffi::PyObject {
            unsafe { $binding.invoke(args, nargs, keywords, |$py, $values| $body) }
        }
    };
}

callback! {
    standard_b64encode, schema::STANDARD_B64ENCODE, |py, values| {
        return_bound(
            py,
            super::standard_b64encode(py, raw_argument(py, &values[0])),
        )
    }
}

callback! {
    standard_b64encode_into, schema::STANDARD_B64ENCODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            super::standard_b64encode_into(py, raw_argument(py, &values[0]), output)
        })();
        return_usize(py, result)
    }
}

callback! {
    urlsafe_b64encode, schema::URLSAFE_B64ENCODE, |py, values| {
        let result = truthy_argument(py, values[1], true)
            .and_then(|padded| super::urlsafe_b64encode(py, raw_argument(py, &values[0]), padded));
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_into, schema::URLSAFE_B64ENCODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let padded = truthy_argument(py, values[2], true)?;
            super::urlsafe_b64encode_into(py, raw_argument(py, &values[0]), output, padded)
        })();
        return_usize(py, result)
    }
}

callback! {
    b64encode, schema::B64ENCODE, |py, values| {
        let result = (|| {
            let padded = truthy_argument(py, values[2], true)?;
            let wrapcol = if values[3].is_null() {
                0
            } else {
                raw_argument(py, &values[3]).extract::<i128>()?
            };
            super::b64encode(
                py,
                raw_argument(py, &values[0]),
                optional_argument(py, &values[1]),
                padded,
                wrapcol,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_batch, schema::B64ENCODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::b64encode_batch(py, items, optional_argument(py, &values[1]))
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_batch_into, schema::B64ENCODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::b64encode_batch_into(py, items, outputs, optional_argument(py, &values[2]))
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64encode_batch, schema::STANDARD_B64ENCODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::standard_b64encode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64encode_batch_into, schema::STANDARD_B64ENCODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::standard_b64encode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_batch, schema::URLSAFE_B64ENCODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::urlsafe_b64encode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64encode_batch_into, schema::URLSAFE_B64ENCODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::urlsafe_b64encode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

callback! {
    b64encode_into, schema::B64ENCODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let padded = truthy_argument(py, values[3], true)?;
            let wrapcol = if values[4].is_null() {
                0
            } else {
                raw_argument(py, &values[4]).extract::<i128>()?
            };
            super::b64encode_into(
                py,
                raw_argument(py, &values[0]),
                output,
                optional_argument(py, &values[2]),
                padded,
                wrapcol,
            )
        })();
        return_usize(py, result)
    }
}

callback! {
    b64decode, schema::B64DECODE, |py, values| {
        let result = (|| {
            let validate = if values[2].is_null() {
                None
            } else {
                Some(truthy_argument(py, values[2], false)?)
            };
            let padded = truthy_argument(py, values[3], true)?;
            let canonical = truthy_argument(py, values[5], false)?;
            super::b64decode(
                py,
                raw_argument(py, &values[0]),
                optional_argument(py, &values[1]),
                validate,
                padded,
                provided_argument(py, &values[4]),
                canonical,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64decode, schema::STANDARD_B64DECODE, |py, values| {
        return_bound(
            py,
            super::standard_b64decode(py, raw_argument(py, &values[0])),
        )
    }
}

callback! {
    standard_b64decode_into, schema::STANDARD_B64DECODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            super::standard_b64decode_into(py, raw_argument(py, &values[0]), output)
        })();
        return_usize(py, result)
    }
}

callback! {
    standard_b64decode_batch, schema::STANDARD_B64DECODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::standard_b64decode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

callback! {
    standard_b64decode_batch_into, schema::STANDARD_B64DECODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::standard_b64decode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_batch, schema::URLSAFE_B64DECODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            super::urlsafe_b64decode_batch(py, items)
        })();
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_batch_into, schema::URLSAFE_B64DECODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            super::urlsafe_b64decode_batch_into(py, items, outputs)
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_batch, schema::B64DECODE_BATCH, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let validate = truthy_argument(py, values[2], false)?;
            super::b64decode_batch(py, items, optional_argument(py, &values[1]), validate)
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_batch_into, schema::B64DECODE_BATCH_INTO, |py, values| {
        let result = (|| {
            let items = raw_argument(py, &values[0]).cast::<PyList>()?;
            let outputs = raw_argument(py, &values[1]).cast::<PyList>()?;
            let validate = truthy_argument(py, values[3], false)?;
            super::b64decode_batch_into(
                py,
                items,
                outputs,
                optional_argument(py, &values[2]),
                validate,
            )
        })();
        return_bound(py, result)
    }
}

callback! {
    b64decode_into, schema::B64DECODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let validate = if values[3].is_null() {
                None
            } else {
                Some(truthy_argument(py, values[3], false)?)
            };
            let padded = truthy_argument(py, values[4], true)?;
            let canonical = truthy_argument(py, values[6], false)?;
            super::b64decode_into(
                py,
                raw_argument(py, &values[0]),
                output,
                optional_argument(py, &values[2]),
                validate,
                padded,
                provided_argument(py, &values[5]),
                canonical,
            )
        })();
        return_usize(py, result)
    }
}

callback! {
    urlsafe_b64decode, schema::URLSAFE_B64DECODE, |py, values| {
        let default = !python_at_least(py, (3, 15));
        let result = truthy_argument(py, values[1], default)
            .and_then(|padded| super::urlsafe_b64decode(py, raw_argument(py, &values[0]), padded));
        return_bound(py, result)
    }
}

callback! {
    urlsafe_b64decode_into, schema::URLSAFE_B64DECODE_INTO, |py, values| {
        let result = (|| {
            let output = raw_argument(py, &values[1]).cast::<PyByteArray>()?;
            let default = !python_at_least(py, (3, 15));
            let padded = truthy_argument(py, values[2], default)?;
            super::urlsafe_b64decode_into(py, raw_argument(py, &values[0]), output, padded)
        })();
        return_usize(py, result)
    }
}
