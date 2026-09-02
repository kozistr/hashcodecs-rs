//! Shared validation and alias protection for Base64 batch output buffers.

use std::collections::HashSet;

use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyList, PyString};

use super::super::buffer::{BytesLike, ascii_or_bytes_owned, contiguous_bytes_like_owned};
use super::super::objects::list_items;

pub(in crate::bindings::base64) fn batch_results<T>(length: usize) -> PyResult<Vec<T>> {
    let mut results = Vec::new();
    results
        .try_reserve_exact(length)
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    Ok(results)
}

pub(in crate::bindings::base64) struct BatchOutputs<'py> {
    outputs: Vec<Bound<'py, PyByteArray>>,
    identities: HashSet<*mut ffi::PyObject>,
}

impl<'py> BatchOutputs<'py> {
    pub(in crate::bindings::base64) fn get(&self, index: usize) -> &Bound<'py, PyByteArray> {
        &self.outputs[index]
    }

    fn contains_identity(&self, identity: *mut ffi::PyObject) -> bool {
        self.identities.contains(&identity)
    }
}

pub(in crate::bindings::base64) fn batch_outputs<'py>(
    items_length: usize,
    outputs: &Bound<'py, PyList>,
) -> PyResult<BatchOutputs<'py>> {
    if outputs.len() != items_length {
        return Err(PyValueError::new_err(
            "items and outputs must have the same length",
        ));
    }

    let mut parsed = batch_results(outputs.len())?;
    let mut identities = HashSet::new();
    identities
        .try_reserve(outputs.len())
        .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
    for (index, output) in list_items(outputs)?.into_iter().enumerate() {
        let output = output
            .cast_into::<PyByteArray>()
            .map_err(|_| PyTypeError::new_err(format!("outputs[{index}] must be a bytearray")))?;
        if !identities.insert(output.as_ptr()) {
            return Err(PyValueError::new_err(
                "outputs must contain distinct bytearrays",
            ));
        }
        parsed.push(output);
    }
    Ok(BatchOutputs {
        outputs: parsed,
        identities,
    })
}

#[derive(Clone, Copy)]
pub(in crate::bindings::base64) enum BatchInputKind {
    Contiguous,
    AsciiOrBytes,
}

pub(in crate::bindings::base64) type PreparedBatchInput<'py> = Result<BytesLike<'py, 'py>, PyErr>;

/// Convert only inputs that destination writes can invalidate.
/// Exact immutable values and independent bytearrays remain on the single-pass path.
/// The conversion handles these inputs:
///
/// * aliased bytearrays
/// * string subclasses that can override encoding
/// * other buffer exporters
pub(in crate::bindings::base64) fn prepare_batch_inputs<'py>(
    items: &[Bound<'py, PyAny>],
    outputs: &BatchOutputs<'py>,
    kind: BatchInputKind,
) -> PyResult<Vec<(usize, PreparedBatchInput<'py>)>> {
    let needs_preparation = |item: &Bound<'py, PyAny>| {
        if PyBytes::is_exact_type_of(item) {
            return false;
        }
        if matches!(kind, BatchInputKind::AsciiOrBytes) && item.is_instance_of::<PyString>() {
            return !PyString::is_exact_type_of(item);
        }
        if PyByteArray::is_exact_type_of(item) {
            return outputs.contains_identity(item.as_ptr());
        }
        true
    };

    if !items.iter().any(&needs_preparation) {
        return Ok(Vec::new());
    }

    // Acquiring a buffer or encoding a string subclass can run arbitrary
    // Python. Keep every acquired value alive until every exporter has run,
    // then snapshot those uncommon inputs before any destination write.
    let mut prepared = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if !needs_preparation(item) {
            continue;
        }
        let input = match kind {
            BatchInputKind::Contiguous => contiguous_bytes_like_owned(item, "s"),
            BatchInputKind::AsciiOrBytes => ascii_or_bytes_owned(item, "s"),
        };
        match input {
            Ok(input) => {
                prepared
                    .try_reserve(1)
                    .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
                prepared.push((index, Ok(input)));
            }
            Err(error) => {
                prepared
                    .try_reserve(1)
                    .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
                prepared.push((index, Err(error)));
                break;
            }
        }
    }

    // Releasing a Python buffer can itself invoke exporter code, so snapshot
    // only after every acquired input is still alive.
    let mut snapshots = batch_results(prepared.len())?;
    for (_, input) in &prepared {
        let snapshot = match input {
            Ok(input) => input.snapshot_if(true)?,
            Err(_) => None,
        };
        snapshots.push(snapshot);
    }

    for ((_, input), snapshot) in prepared.iter_mut().zip(snapshots) {
        if let (Ok(input), Some(snapshot)) = (input, snapshot) {
            *input = BytesLike::OwnedVec(snapshot);
        }
    }
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::batch_results;

    #[test]
    fn oversized_batch_capacity_is_an_error() {
        assert!(batch_results::<u8>(usize::MAX).is_err());
    }
}
