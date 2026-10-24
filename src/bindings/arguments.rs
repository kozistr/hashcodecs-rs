use std::ffi::{CStr, c_char};
use std::ptr;

use pyo3::ffi;

/// # Safety
/// Argument pointers must refer to live CPython objects or permitted absent-value nulls; FASTCALL
/// arrays and keyword tuples must cover their declared lengths while Python is attached.
#[inline(always)]
unsafe fn keyword_matches(keyword: *mut ffi::PyObject, parameter: &CStr) -> bool {
    #[cfg(all(Py_3_12, not(any(Py_LIMITED_API, PyPy, GraalPy))))]
    // SAFETY: CPython provides a live Unicode keyword and the bounded comparison uses its reported
    // character length.
    unsafe {
        let name = parameter.to_bytes();

        // Keyword strings are immutable, including on free-threaded CPython.
        // Reject different lengths before comparing the schema's ASCII names.
        if ffi::PyUnicode_GET_LENGTH(keyword) as usize != name.len() {
            return false;
        }

        #[cfg(not(Py_3_14))]
        {
            if ffi::PyUnicode_IS_ASCII(keyword) != 0 {
                std::slice::from_raw_parts(ffi::PyUnicode_1BYTE_DATA(keyword), name.len()) == name
            } else {
                // C extensions can store ASCII text in a wider Unicode allocation.
                ffi::PyUnicode_CompareWithASCIIString(keyword, parameter.as_ptr()) == 0
            }
        }

        #[cfg(Py_3_14)]
        {
            // PyO3 leaves the Unicode state bitfield opaque from 3.14 onward.
            // Use the public length-aware equality API on these interpreters.
            ffi::PyUnicode_EqualToUTF8AndSize(keyword, parameter.as_ptr(), name.len() as isize) != 0
                || ffi::PyUnicode_CompareWithASCIIString(keyword, parameter.as_ptr()) == 0
        }
    }

    #[cfg(not(all(Py_3_12, not(any(Py_LIMITED_API, PyPy, GraalPy)))))]
    // SAFETY: CPython provides a live Unicode keyword and the bounded comparison uses its reported
    // character length.
    unsafe {
        ffi::PyUnicode_CompareWithASCIIString(keyword, parameter.as_ptr()) == 0
    }
}

/// # Safety
/// Argument pointers must refer to live CPython objects or permitted absent-value nulls; FASTCALL
/// arrays and keyword tuples must cover their declared lengths while Python is attached.
#[inline(always)]
pub(super) unsafe fn parse_raw_arguments<const N: usize>(
    args: *const *mut ffi::PyObject,
    nargs: isize,
    keywords: *mut ffi::PyObject,
    function_name: *const c_char,
    parameter_names: [&CStr; N],
    max_positional: usize,
    required: usize,
) -> Option<[*mut ffi::PyObject; N]> {
    let nargs = nargs as usize;

    if nargs > max_positional {
        // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
        // values; checked indices and keyword names bound each access.
        unsafe {
            ffi::PyErr_Format(
                ffi::PyExc_TypeError,
                c"%s() takes at most %zu positional arguments (%zu given)".as_ptr(),
                function_name,
                max_positional,
                nargs,
            );
        }

        return None;
    }

    let mut values = [ptr::null_mut(); N];

    for (index, value) in values.iter_mut().take(nargs).enumerate() {
        // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
        // values; checked indices and keyword names bound each access.
        *value = unsafe { *args.add(index) };
    }

    let keyword_count = if keywords.is_null() {
        0
    } else {
        // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
        // values; checked indices and keyword names bound each access.
        unsafe { ffi::PyTuple_GET_SIZE(keywords) as usize }
    };

    for keyword_index in 0..keyword_count {
        // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
        // values; checked indices and keyword names bound each access.
        let keyword = unsafe { ffi::PyTuple_GET_ITEM(keywords, keyword_index as isize) };
        // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
        // values; checked indices and keyword names bound each access.
        let value = unsafe { *args.add(nargs + keyword_index) };
        // Valid keywords follow the positional arguments. Search those slots
        // first, then check the filled slots to preserve duplicate diagnostics.
        let parameter_index = (nargs..N)
            .chain(0..nargs)
            // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
            // values; checked indices and keyword names bound each access.
            .find(|&index| unsafe { keyword_matches(keyword, parameter_names[index]) });

        let Some(parameter_index) = parameter_index else {
            // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
            // values; checked indices and keyword names bound each access.
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() got an unexpected keyword argument '%U'".as_ptr(),
                    function_name,
                    keyword,
                );
            }

            return None;
        };

        if !values[parameter_index].is_null() {
            // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
            // values; checked indices and keyword names bound each access.
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() got multiple values for argument '%s'".as_ptr(),
                    function_name,
                    parameter_names[parameter_index].as_ptr(),
                );
            }

            return None;
        }

        values[parameter_index] = value;
    }

    for index in 0..required {
        if values[index].is_null() {
            // SAFETY: The FASTCALL contract provides nargs positional slots followed by the keyword
            // values; checked indices and keyword names bound each access.
            unsafe {
                ffi::PyErr_Format(
                    ffi::PyExc_TypeError,
                    c"%s() missing required argument '%s'".as_ptr(),
                    function_name,
                    parameter_names[index].as_ptr(),
                );
            }

            return None;
        }
    }

    Some(values)
}

/// # Safety
/// Argument pointers must refer to live CPython objects or permitted absent-value nulls; FASTCALL
/// arrays and keyword tuples must cover their declared lengths while Python is attached.
#[inline]
pub(super) unsafe fn seed_u32(seed: *mut ffi::PyObject) -> Option<u32> {
    // SAFETY: The callback retains the live argument while the attached CPython integer conversion
    // reports overflow through the error indicator.
    let value = unsafe { seed_u64(seed) }?;

    let Ok(value) = u32::try_from(value) else {
        // SAFETY: The callback retains the live argument while the attached CPython integer
        // conversion reports overflow through the error indicator.
        unsafe {
            ffi::PyErr_SetString(
                ffi::PyExc_OverflowError,
                c"seed does not fit in uint32".as_ptr(),
            );
        }

        return None;
    };

    Some(value)
}

/// # Safety
/// Argument pointers must refer to live CPython objects or permitted absent-value nulls; FASTCALL
/// arrays and keyword tuples must cover their declared lengths while Python is attached.
#[inline]
pub(super) unsafe fn seed_u64(seed: *mut ffi::PyObject) -> Option<u64> {
    if seed.is_null() {
        return Some(0);
    }

    // SAFETY: The callback retains the live argument while the attached CPython integer conversion
    // reports overflow through the error indicator.
    let value = unsafe { ffi::PyLong_AsUnsignedLongLong(seed) };

    // The conversion reports failure with ULLONG_MAX; all other values succeed.
    // Check the exception only for the sentinel, which is also a valid seed.
    // SAFETY: The callback retains the live argument while the attached CPython integer conversion
    // reports overflow through the error indicator.
    if value != u64::MAX || unsafe { ffi::PyErr_Occurred() }.is_null() {
        Some(value)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyo3::prelude::*;

    #[test]
    fn match_unicode_keywords() {
        Python::initialize();
        // SAFETY: The test retains the Python objects and stays attached; buffer access is
        // synchronized by the enclosing guards.
        Python::attach(|py| unsafe {
            for max_char in [0x7f, 0xff, 0xffff, 0x10ffff] {
                let keyword =
                    Bound::<PyAny>::from_owned_ptr_or_err(py, ffi::PyUnicode_New(4, max_char))
                        .unwrap();

                for (index, character) in b"seed".iter().enumerate() {
                    assert_eq!(
                        ffi::PyUnicode_WriteChar(
                            keyword.as_ptr(),
                            index as isize,
                            (*character).into()
                        ),
                        0
                    );
                }

                assert!(keyword_matches(keyword.as_ptr(), c"seed"));
                assert!(!keyword_matches(keyword.as_ptr(), c"s"));
                assert!(!keyword_matches(keyword.as_ptr(), c"sead"));
            }
        });
    }
}
