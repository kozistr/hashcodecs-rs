use pyo3::marker::Ungil;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::digest::{hex_digest, x64_128_digest, x86_128_digest};
use crate::bindings::buffer::{BytesLike, bytes_like};
use crate::bindings::runtime::MURMUR3_DETACH_THRESHOLD;
use crate::murmur3::{Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128};

fn with_input<T: Ungil>(
    py: Python<'_>,
    input: &BytesLike<'_, '_>,
    operation: impl Ungil + Send + FnOnce(&[u8]) -> T,
) -> T {
    let detach = input.detach_safe() && input.len() >= MURMUR3_DETACH_THRESHOLD;
    unsafe {
        input.with_bytes(|input| {
            if detach {
                py.detach(|| operation(input))
            } else {
                operation(input)
            }
        })
    }
}

macro_rules! define_python_hasher {
    (
        $class:ident,
        $state:ty,
        $python_name:literal,
        $summary:literal,
        $examples:literal,
        $digest_size:literal,
        $block_size:literal,
        $digest:expr $(,)?
    ) => {
        #[doc = concat!("Incremental MurmurHash3 ", $summary, " hasher.\n")]
        #[doc = "Args:\n    data: Optional initial bytes-like data.\n    seed: Initial unsigned 32-bit seed.\n"]
        #[doc = concat!("Examples:\n", $examples)]
        #[pyclass(
            name = $python_name,
            module = "hashcodecs.murmur3",
            skip_from_py_object
        )]
        #[derive(Clone)]
        pub(in crate::bindings) struct $class {
            state: $state,
        }

        #[pymethods]
        impl $class {
            /// Initialize an incremental hash state.
            ///
            /// Args:
            ///     data: Optional initial bytes-like data.
            ///     seed: Initial unsigned 32-bit seed.
            ///
            /// Raises:
            ///     TypeError: data is not bytes-like or seed is not an integer.
            ///     OverflowError: seed is outside 0 <= seed < 2**32.
            #[new]
            #[pyo3(signature = (data=None, seed=0))]
            fn new(py: Python<'_>, data: Option<&Bound<'_, PyAny>>, seed: u32) -> PyResult<Self> {
                let mut state = <$state>::new(seed);
                if let Some(data) = data {
                    let input = bytes_like(py, data, "data")?;
                    with_input(py, &input, |input| state.update(input));
                }
                Ok(Self { state })
            }

            /// Add bytes to the hash state.
            ///
            /// Args:
            ///     data: Bytes-like data to add.
            ///
            /// Raises:
            ///     TypeError: data is not bytes-like.
            fn update(&mut self, py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<()> {
                let input = bytes_like(py, data, "data")?;
                with_input(py, &input, |input| self.state.update(input));
                Ok(())
            }

            /// Return the current digest without changing the state.
            fn digest<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
                let digest = ($digest)(&self.state);
                PyBytes::new(py, &digest)
            }

            /// Return the current digest as lowercase hexadecimal text.
            fn hexdigest(&self) -> String {
                hex_digest(&($digest)(&self.state))
            }

            /// Return an independent copy of the current hash state.
            fn copy(&self) -> Self {
                self.clone()
            }

            #[getter]
            /// The digest size in bytes.
            const fn digest_size(&self) -> usize {
                $digest_size
            }

            #[getter]
            /// The algorithm block size in bytes.
            const fn block_size(&self) -> usize {
                $block_size
            }

            #[getter]
            /// The algorithm name.
            const fn name(&self) -> &'static str {
                $python_name
            }
        }
    };
}

define_python_hasher!(
    PyMurmur3X86Hasher32,
    Murmur3X86Hasher32,
    "murmur3_x86_32",
    "x86 32-bit",
    "    >>> hasher = murmur3_x86_32(b'hello', seed=7)\n    >>> hasher.update(b' world')\n    >>> hasher.hexdigest() == hasher.digest().hex()\n    True",
    4,
    4,
    |state: &Murmur3X86Hasher32| state.digest().to_le_bytes(),
);

define_python_hasher!(
    PyMurmur3X86Hasher128,
    Murmur3X86Hasher128,
    "murmur3_x86_128",
    "x86 128-bit",
    "    >>> hasher = murmur3_x86_128(b'hello', seed=7)\n    >>> hasher.update(b' world')\n    >>> len(hasher.digest())\n    16",
    16,
    16,
    |state: &Murmur3X86Hasher128| x86_128_digest(state.digest()),
);

define_python_hasher!(
    PyMurmur3X64Hasher128,
    Murmur3X64Hasher128,
    "murmur3_x64_128",
    "x64 128-bit",
    "    >>> hasher = murmur3_x64_128(b'hello', seed=7)\n    >>> checkpoint = hasher.copy()\n    >>> hasher.update(b' world')\n    >>> hasher.digest() != checkpoint.digest()\n    True",
    16,
    16,
    |state: &Murmur3X64Hasher128| x64_128_digest(state.digest()),
);
