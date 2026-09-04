use core::ptr;
use std::ffi::CStr;

use pyo3::ffi;
use pyo3::prelude::*;

use crate::bindings::arguments::parse_raw_arguments;
use crate::bindings::base64::python_at_least;
use crate::bindings::runtime::{METHOD_FLAGS, catch_unwind_callback};

pub(super) type Callback = unsafe extern "C" fn(
    *mut ffi::PyObject,
    *const *mut ffi::PyObject,
    isize,
    *mut ffi::PyObject,
) -> *mut ffi::PyObject;

#[derive(Clone, Copy)]
pub(super) enum DefaultValue {
    Required,
    Missing,
    None,
    Bool(bool),
    I128(i128),
    VersionedBool {
        before: bool,
        since: (u8, u8),
        after: bool,
    },
}

#[derive(Clone, Copy)]
pub(super) struct Argument {
    value: *mut ffi::PyObject,
    default: DefaultValue,
}

impl Argument {
    #[inline(always)]
    pub(super) const fn new(value: *mut ffi::PyObject, default: DefaultValue) -> Self {
        Self { value, default }
    }

    #[inline(always)]
    pub(super) const fn as_ptr(self) -> *mut ffi::PyObject {
        self.value
    }

    #[inline(always)]
    pub(super) fn raw<'a, 'py>(&'a self, py: Python<'py>) -> &'a Bound<'py, PyAny> {
        assert!(
            !self.value.is_null(),
            "required binding argument must be present"
        );
        unsafe { Bound::ref_from_ptr(py, &self.value) }
    }

    #[inline(always)]
    pub(super) fn optional<'a, 'py>(&'a self, py: Python<'py>) -> Option<&'a Bound<'py, PyAny>> {
        if self.value.is_null() || self.value == unsafe { ffi::Py_None() } {
            None
        } else {
            Some(self.raw(py))
        }
    }

    #[inline(always)]
    pub(super) fn provided<'a, 'py>(&'a self, py: Python<'py>) -> Option<&'a Bound<'py, PyAny>> {
        (!self.value.is_null()).then(|| self.raw(py))
    }

    #[inline(always)]
    fn default_bool(self, py: Python<'_>) -> bool {
        match self.default {
            DefaultValue::Bool(value) => value,
            DefaultValue::VersionedBool {
                before,
                since,
                after,
            } => {
                if python_at_least(py, since) {
                    after
                } else {
                    before
                }
            }
            _ => unreachable!("binding argument does not have a boolean default"),
        }
    }

    #[inline(always)]
    pub(super) fn truthy(self, py: Python<'_>) -> PyResult<bool> {
        if self.value.is_null() {
            return Ok(self.default_bool(py));
        }
        let truthy = unsafe { ffi::PyObject_IsTrue(self.value) };
        if truthy == -1 {
            Err(PyErr::fetch(py))
        } else {
            Ok(truthy != 0)
        }
    }

    #[inline(always)]
    pub(super) fn optional_truthy(self, py: Python<'_>) -> PyResult<Option<bool>> {
        if self.value.is_null() && matches!(self.default, DefaultValue::Missing) {
            Ok(None)
        } else {
            self.truthy(py).map(Some)
        }
    }

    #[inline(always)]
    pub(super) fn extract_i128(self, py: Python<'_>) -> PyResult<i128> {
        if self.value.is_null() {
            return match self.default {
                DefaultValue::I128(value) => Ok(value),
                _ => unreachable!("binding argument does not have an integer default"),
            };
        }
        self.raw(py).extract::<i128>()
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
    default: &'static CStr,
    python_315: Option<&'static CStr>,
}

impl Documentation {
    fn select(self, version: (u8, u8)) -> &'static CStr {
        if version >= (3, 15) {
            self.python_315.unwrap_or(self.default)
        } else {
            self.default
        }
    }
}

struct Binding<const N: usize> {
    name: &'static CStr,
    callback: Callback,
    parser: Parser<N>,
    documentation: Documentation,
}

impl<const N: usize> Binding<N> {
    #[inline(always)]
    unsafe fn invoke(
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

    unsafe fn register(
        &self,
        methods: *mut ffi::PyMethodDef,
        method_count: &mut usize,
        version: (u8, u8),
    ) {
        let method = ffi::PyMethodDef {
            ml_name: self.name.as_ptr(),
            ml_meth: ffi::PyMethodDefPointer {
                PyCFunctionFastWithKeywords: self.callback,
            },
            ml_flags: METHOD_FLAGS,
            ml_doc: self.documentation.select(version).as_ptr(),
        };
        unsafe { methods.add(*method_count).write(method) };
        *method_count += 1;
    }
}

macro_rules! binding {
    (
        $constant:ident: $count:literal {
            name: $name:expr,
            callback: $callback:path,
            parameters: [$($parameter:expr),* $(,)?],
            max_positional: $max_positional:expr,
            required: $required:expr,
            documentation: $documentation:expr,
            python_315_documentation: $python_315_documentation:expr $(,)?
        }
    ) => {
        const $constant: Binding<$count> = Binding {
            name: $name,
            callback: $callback,
            parser: Parser {
                parameters: [$($parameter),*],
                max_positional: $max_positional,
                required: $required,
            },
            documentation: Documentation {
                default: $documentation,
                python_315: $python_315_documentation,
            },
        };
    };
}

include!("../../generated/rust/binding_schema.rs");
