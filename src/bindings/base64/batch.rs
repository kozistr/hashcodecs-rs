//! Batch entry points, retained inputs, and cross-item alias snapshots.

use std::collections::HashSet;
use std::sync::OnceLock;

use pyo3::PyTypeInfo;
use pyo3::exceptions::{PyMemoryError, PyTypeError, PyValueError};
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyByteArray, PyBytes, PyInt, PyList, PyMemoryView, PyString};

#[cfg(not(Py_GIL_DISABLED))]
use super::encode::encode_exact;
use super::encode::{encode_parsed, encode_parsed_into};
use super::policy::{DecodePolicy, PreparedDecoder};
use crate::bindings::buffer::{
    BufferRange, BytesLike, ascii_or_bytes, ascii_or_bytes_owned, contiguous_bytes_like,
    contiguous_bytes_like_owned,
};
use crate::bindings::compatibility::parse_altchars;
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::exact_bytes_up_to;
use crate::bindings::objects::{batch_results, list_from_fn, list_items};

const BATCH_TOO_LARGE: &str = "Base64 batch is too large";

struct BatchOutputs<'py> {
    outputs: Vec<Bound<'py, PyByteArray>>,
    identities: HashSet<*mut ffi::PyObject>,
    ranges: OnceLock<Vec<BufferRange>>,
}

impl<'py> BatchOutputs<'py> {
    fn get(&self, index: usize) -> &Bound<'py, PyByteArray> {
        &self.outputs[index]
    }

    fn contains_identity(&self, identity: *mut ffi::PyObject) -> bool {
        self.identities.contains(&identity)
    }

    fn ranges(&self) -> PyResult<&[BufferRange]> {
        if let Some(ranges) = self.ranges.get() {
            return Ok(ranges);
        }

        let mut ranges = batch_results(self.outputs.len(), BATCH_TOO_LARGE)?;
        ranges.extend(self.outputs.iter().filter_map(BufferRange::for_bytearray));
        ranges.sort_unstable_by_key(|range| range.start());
        let _ = self.ranges.set(ranges);
        Ok(self
            .ranges
            .get()
            .expect("Base64 output ranges were initialized"))
    }

    fn snapshot_alias(&self, input: &BytesLike<'py, 'py>) -> PyResult<Option<Vec<u8>>> {
        if input
            .bytearray_identity()
            .is_some_and(|identity| self.contains_identity(identity))
        {
            return input.snapshot_if(true);
        }

        let Some(input_range) = input.buffer_range() else {
            return Ok(None);
        };

        let ranges = self.ranges()?;
        let index = ranges.partition_point(|output| output.start() < input_range.end());
        let aliases_output = index != 0 && ranges[index - 1].overlaps(input_range);

        input.snapshot_if(aliases_output)
    }
}

fn batch_outputs<'py>(
    items_length: usize,
    outputs: &Bound<'py, PyList>,
) -> PyResult<BatchOutputs<'py>> {
    if outputs.len() != items_length {
        return Err(PyValueError::new_err(
            "items and outputs must have the same length",
        ));
    }

    let mut parsed = batch_results(outputs.len(), BATCH_TOO_LARGE)?;
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
        ranges: OnceLock::new(),
    })
}

#[derive(Clone, Copy)]
enum BatchInputKind {
    Contiguous,
    AsciiOrBytes,
}

type PreparedBatchInput<'py> = Result<BytesLike<'py, 'py>, PyErr>;

#[derive(Clone, Copy, Eq, PartialEq)]
enum SnapshotPolicy {
    AliasesOnly,
    ReleaseBeforeWrite,
}

type PreparedBatchItem<'py> = (usize, PreparedBatchInput<'py>, SnapshotPolicy);

fn stage_snapshots<'py>(
    prepared: &mut [PreparedBatchItem<'py>],
    mut snapshot: impl FnMut(&BytesLike<'py, 'py>, SnapshotPolicy) -> PyResult<Option<Vec<u8>>>,
) -> PyResult<()> {
    let mut snapshots = batch_results(prepared.len(), BATCH_TOO_LARGE)?;

    for (_, input, policy) in prepared.iter() {
        snapshots.push(match input {
            Ok(input) => snapshot(input, *policy)?,
            Err(_) => None,
        })
    }

    for ((_, input, _), snapshot) in prepared.iter_mut().zip(snapshots) {
        if let (Ok(input), Some(snapshot)) = (input, snapshot) {
            *input = BytesLike::OwnedVec(snapshot);
        }
    }

    Ok(())
}

/// Stabilize inputs whose release or destination writes can invalidate them.
/// Exact immutable values and independent bytearrays remain on the single-pass path.
/// The function handles these inputs:
///
/// * aliased bytearrays
/// * exact memoryviews that overlap any destination
/// * string subclasses and custom buffer exporters with reentrant release hooks
fn prepare_batch_inputs<'py>(
    items: &[Bound<'py, PyAny>],
    outputs: &BatchOutputs<'py>,
    kind: BatchInputKind,
) -> PyResult<Vec<PreparedBatchItem<'py>>> {
    let snapshot_policy = |item: &Bound<'py, PyAny>| {
        if PyBytes::is_exact_type_of(item) {
            return None;
        }
        if matches!(kind, BatchInputKind::AsciiOrBytes) && item.is_instance_of::<PyString>() {
            return (!PyString::is_exact_type_of(item))
                .then_some(SnapshotPolicy::ReleaseBeforeWrite);
        }
        if PyByteArray::is_exact_type_of(item) {
            return outputs
                .contains_identity(item.as_ptr())
                .then_some(SnapshotPolicy::AliasesOnly);
        }
        if PyMemoryView::is_exact_type_of(item) {
            return Some(SnapshotPolicy::AliasesOnly);
        }
        Some(SnapshotPolicy::ReleaseBeforeWrite)
    };

    if !items.iter().any(|item| snapshot_policy(item).is_some()) {
        return Ok(Vec::new());
    }

    // Acquiring a buffer or encoding a string subclass can run arbitrary
    // Python. Keep every acquired value alive until every exporter has run,
    // then stabilize those uncommon inputs before any destination write.
    let mut prepared = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(policy) = snapshot_policy(item) else {
            continue;
        };
        let input = match kind {
            BatchInputKind::Contiguous => contiguous_bytes_like_owned(item, "s"),
            BatchInputKind::AsciiOrBytes => ascii_or_bytes_owned(item, "s"),
        };
        match input {
            Ok(input) => {
                prepared
                    .try_reserve(1)
                    .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
                prepared.push((index, Ok(input), policy));
            }
            Err(error) => {
                prepared
                    .try_reserve(1)
                    .map_err(|_| PyMemoryError::new_err("Base64 batch is too large"))?;
                prepared.push((index, Err(error), policy));
                break;
            }
        }
    }

    // Release custom exporters before capturing output addresses. Their release
    // hooks can resize a destination. Exact memoryviews remain owned by items.
    stage_snapshots(&mut prepared, |input, policy| {
        // ReleaseBeforeWrite also covers a temporary exact memoryview returned by
        // a string subclass. The items list does not retain that memoryview.
        let needed = input.buffer_release_may_reenter()
            || (policy == SnapshotPolicy::ReleaseBeforeWrite && input.has_borrowed_buffer());

        input.snapshot_if(needed)
    })?;

    // Capture destination ranges after every reentrant release, then stage all
    // alias snapshots before dropping any remaining buffer export.
    stage_snapshots(&mut prepared, |input, _| outputs.snapshot_alias(input))?;
    Ok(prepared)
}

#[cfg(not(Py_GIL_DISABLED))]
const EXACT_BYTES_BATCH_MAX: usize = 256;

/// Encode each bytes-like item and return the results in input order.
///
/// ``items`` must be a list. ``altchars`` applies to every item.
/// The function stops at the first error and discards the partial result list.
/// The function uses one thread. It releases the GIL for each immutable item of at least 256 KiB.
/// It retains the GIL for smaller or mutable items. Do not change ``items`` during the call.
pub(super) fn b64encode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_parsed(py, items, altchars)
}

pub(super) fn b64encode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(items) = exact_bytes_up_to(items, EXACT_BYTES_BATCH_MAX)? {
        // Validation retains every input before allocating the output list.
        // Creating a GC-tracked Python object can run finalizers which mutate
        // the original list.
        let length = items.len();
        let mut items = items.into_iter();
        return list_from_fn(py, length, |_| {
            let item = items.next().expect("batch item count is exact");
            encode_exact(py, item.as_bytes(), altchars, true, None)
        });
    }
    let items = list_items(items)?;
    let length = items.len();
    let mut items = items.into_iter();
    list_from_fn(py, length, |_| {
        encode_parsed(
            py,
            &items.next().expect("batch item count is exact"),
            altchars,
            true,
            None,
        )
    })
}

/// Encode each item into its matching bytearray and return the byte counts.
///
/// ``items`` and ``outputs`` must be lists of equal length. Each destination must be a different bytearray.
/// Each destination keeps its size. The function changes only the written prefix.
/// The function stops at the first error. It does not restore destinations that it changed.
/// The function retains the GIL because outputs are mutable.
/// It copies all inputs that overlap a destination before it writes to the first destination.
pub(super) fn b64encode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, false)?;
    b64encode_batch_into_parsed(py, items, outputs, altchars)
}

pub(super) fn b64encode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut prepared = prepare_batch_inputs(&items, &outputs, BatchInputKind::Contiguous)?
        .into_iter()
        .peekable();
    list_from_fn(py, items.len(), |index| {
        let output = outputs.get(index);
        match prepared
            .peek()
            .is_some_and(|(prepared_index, _, _)| *prepared_index == index)
            .then(|| prepared.next().expect("matching prepared input exists").1)
        {
            Some(Ok(input)) => Ok(PyInt::new(
                py,
                encode_parsed_into(&input, output, altchars, true, None)?,
            )),
            Some(Err(error)) => Err(error),
            None => {
                let input = contiguous_bytes_like(&items[index], "s")?;
                Ok(PyInt::new(
                    py,
                    encode_parsed_into(&input, output, altchars, true, None)?,
                ))
            }
        }
    })
}

/// Decode each ASCII string or bytes-like item and return the results in input order.
///
/// ``items`` must be a list. ``altchars`` and ``validate`` apply to every item.
/// The function stops at the first error and discards the partial result list.
/// The function uses one thread. It releases the GIL for each immutable item of at least 256 KiB.
/// It retains the GIL for smaller or mutable items. Do not change ``items`` during the call.
pub(super) fn b64decode_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_parsed(py, items, altchars, validate)
}

pub(super) fn b64decode_batch_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items)?;
    let length = items.len();
    let mut items = items.into_iter();
    let decoder = PreparedDecoder::new(
        py,
        DecodePolicy::new(altchars, Some(validate), true, None, false),
    )?;
    list_from_fn(py, length, |_| {
        let item = items.next().expect("batch item count is exact");
        let input = ascii_or_bytes(py, &item, "s")?;
        decoder.decode_allocating(py, &input)
    })
}

/// Decode each item into its matching bytearray and return the byte counts.
///
/// ``items`` and ``outputs`` must be lists of equal length. Each destination must be a different bytearray.
/// Each destination keeps its size. The function changes only the written prefix.
/// The function stops at the first error. It does not restore destinations that it changed.
/// It can change part of the failing destination. The function retains the GIL because outputs are mutable.
/// It copies all inputs that overlap a destination before it writes to the first destination.
pub(super) fn b64decode_batch_into<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<&Bound<'py, PyAny>>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let altchars = parse_altchars(py, altchars, true)?;
    b64decode_batch_into_parsed(py, items, outputs, altchars, validate)
}

pub(super) fn b64decode_batch_into_parsed<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    outputs: &Bound<'py, PyList>,
    altchars: Option<[u8; 2]>,
    validate: bool,
) -> PyResult<Bound<'py, PyList>> {
    let items = list_items(items)?;
    let outputs = batch_outputs(items.len(), outputs)?;
    let mut prepared = prepare_batch_inputs(&items, &outputs, BatchInputKind::AsciiOrBytes)?
        .into_iter()
        .peekable();
    let decoder = PreparedDecoder::new(
        py,
        DecodePolicy::new(altchars, Some(validate), true, None, false),
    )?;
    list_from_fn(py, items.len(), |index| {
        let output = outputs.get(index);
        match prepared
            .peek()
            .is_some_and(|(prepared_index, _, _)| *prepared_index == index)
            .then(|| prepared.next().expect("matching prepared input exists").1)
        {
            Some(Ok(input)) => Ok(PyInt::new(py, decoder.decode_into(py, &input, output)?)),
            Some(Err(error)) => Err(error),
            None => {
                let input = ascii_or_bytes(py, &items[index], "s")?;
                Ok(PyInt::new(py, decoder.decode_into(py, &input, output)?))
            }
        }
    })
}
