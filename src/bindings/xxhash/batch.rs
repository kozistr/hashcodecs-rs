use std::borrow::Cow;
use std::mem::MaybeUninit;

use pyo3::exceptions::{PyMemoryError, PyValueError};
#[cfg(any(not(Py_3_14), not(Py_GIL_DISABLED)))]
use pyo3::ffi;
use pyo3::prelude::*;
#[cfg(not(Py_GIL_DISABLED))]
use pyo3::types::PyBytes;
use pyo3::types::{PyByteArray, PyInt, PyList};

use crate::bindings::buffer::{BytesLike, bytes_like, with_bytearray};
use crate::bindings::objects::{
    batch_results, bytearray_data, bytearray_size, list_from_fn, list_items,
};
#[cfg(not(Py_GIL_DISABLED))]
use crate::bindings::objects::{exact_bytes_at, exact_bytes_total};
use crate::bindings::runtime::XXH3_DETACH_THRESHOLD;
use crate::xxhash::{xxh3_64_batch_for_each, xxh3_128_batch_for_each};

const BATCH_TOO_LARGE: &str = "XXH3 batch is too large";
// At most 512 bytes for XXH3-128 results; larger batches use a fallible Vec.
const STACK_BATCH_RESULTS: usize = 32;
// Packed output needs only borrowed slice pairs, so keep up to 1 KiB on stack.
#[cfg(not(Py_GIL_DISABLED))]
const PACKED_STACK_INPUTS: usize = 64;

#[cfg(not(Py_GIL_DISABLED))]
struct ExactBytesList<'a, 'py> {
    items: &'a Bound<'py, PyList>,
    total: usize,
}

#[cfg(not(Py_GIL_DISABLED))]
impl<'a, 'py> ExactBytesList<'a, 'py> {
    fn checked(items: &'a Bound<'py, PyList>) -> Option<Self> {
        exact_bytes_total(items).map(|total| Self { items, total })
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn get(&self, index: usize) -> &'a [u8] {
        // `checked` established this invariant, and no operation used through
        // this representation may call Python before all raw borrows are consumed.
        unsafe { exact_bytes_at(self.items, index) }
    }

    fn with_stack<const CAPACITY: usize, T>(
        &self,
        operation: impl FnOnce(&[&'a [u8]]) -> T,
    ) -> Option<T> {
        if self.len() > CAPACITY || self.total >= XXH3_DETACH_THRESHOLD {
            return None;
        }

        let mut inputs = [&[][..]; CAPACITY];
        let inputs = &mut inputs[..self.len()];
        for (index, input) in inputs.iter_mut().enumerate() {
            *input = self.get(index);
        }
        Some(operation(inputs))
    }

    fn borrow(&self) -> PyResult<Vec<&'a [u8]>> {
        let mut inputs = batch_results(self.len(), BATCH_TOO_LARGE)?;
        inputs.extend((0..self.len()).map(|index| self.get(index)));
        Ok(inputs)
    }

    fn retain(&self) -> PyResult<Vec<Bound<'py, PyBytes>>> {
        let mut retained = batch_results(self.len(), BATCH_TOO_LARGE)?;
        for index in 0..self.len() {
            unsafe {
                let item = ffi::PyList_GET_ITEM(self.items.as_ptr(), index as ffi::Py_ssize_t);
                retained.push(
                    Bound::from_borrowed_ptr(self.items.py(), item)
                        .cast_into_unchecked::<PyBytes>(),
                );
            }
        }
        Ok(retained)
    }
}

#[cfg(not(Py_GIL_DISABLED))]
fn borrow_retained<'a, 'py>(retained: &'a [Bound<'py, PyBytes>]) -> PyResult<Vec<&'a [u8]>> {
    let mut inputs = batch_results(retained.len(), BATCH_TOO_LARGE)?;
    inputs.extend(retained.iter().map(|item| item.as_bytes()));
    Ok(inputs)
}

fn parse_batch<'a, 'py>(items: &'a [Bound<'py, PyAny>]) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    let mut inputs = batch_results(items.len(), BATCH_TOO_LARGE)?;
    for item in items {
        let input = bytes_like(item, "items element")?;
        #[cfg(Py_GIL_DISABLED)]
        let input = input.into_stable()?;
        inputs.push(input);
    }
    Ok(inputs)
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= XXH3_DETACH_THRESHOLD
}

fn direct_output_safe(
    inputs: &[BytesLike<'_, '_>],
    output: &Bound<'_, PyByteArray>,
    detach: bool,
) -> bool {
    !detach
        && inputs
            .iter()
            .all(|input| !input.overlaps(output) && !input.buffer_release_may_reenter())
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> PyResult<Vec<&'a [u8]>> {
    let mut borrowed = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
    borrowed.extend(inputs.iter().map(BytesLike::stable_bytes));
    Ok(borrowed)
}

/// # Safety
///
/// `hash` must initialize every destination exactly once before returning.
unsafe fn hash_into_scratch<'a, T: Copy>(
    inputs: &[&[u8]],
    seed: u64,
    scratch: &'a mut [T],
    hash: impl FnOnce(&[&[u8]], u64, &mut [MaybeUninit<T>]),
) -> PyResult<Cow<'a, [T]>> {
    if inputs.len() <= scratch.len() {
        let hashes = &mut scratch[..inputs.len()];
        let destinations =
            unsafe { std::slice::from_raw_parts_mut(hashes.as_mut_ptr().cast(), hashes.len()) };
        hash(inputs, seed, destinations);
        return Ok(Cow::Borrowed(hashes));
    }

    let mut hashes = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
    hash(
        inputs,
        seed,
        &mut hashes.spare_capacity_mut()[..inputs.len()],
    );
    // The caller guarantees that the hash callback initializes every result.
    unsafe { hashes.set_len(inputs.len()) };
    Ok(Cow::Owned(hashes))
}

/// # Safety
///
/// `hash` must initialize every destination exactly once before returning.
unsafe fn batch_hashes<'a, T: Copy + Send + Sync>(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    seed: u64,
    scratch: &'a mut [T],
    hash: impl FnOnce(&[&[u8]], u64, &mut [MaybeUninit<T>]) + Send,
) -> PyResult<Cow<'a, [T]>> {
    // Complete all input reads before allocating Python results. This also
    // avoids retaining small immutable inputs across a GC reentrancy point.
    // The hash callback must only fill native output, without calling Python.
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(exact) = ExactBytesList::checked(items) {
        if exact.len() <= scratch.len() && exact.total < XXH3_DETACH_THRESHOLD {
            let mut inputs = [&[][..]; STACK_BATCH_RESULTS];
            let inputs = &mut inputs[..exact.len()];
            for (index, input) in inputs.iter_mut().enumerate() {
                *input = exact.get(index);
            }
            return unsafe { hash_into_scratch(inputs, seed, scratch, hash) };
        }
        if exact.total < XXH3_DETACH_THRESHOLD {
            let inputs = exact.borrow()?;
            return unsafe { hash_into_scratch(&inputs, seed, scratch, hash) };
        }

        let retained = exact.retain()?;
        let inputs = borrow_retained(&retained)?;
        return py.detach(|| unsafe { hash_into_scratch(&inputs, seed, scratch, hash) });
    }

    let items = list_items(items)?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    let inputs = borrow_batch(&parsed)?;
    if detach {
        py.detach(|| unsafe { hash_into_scratch(&inputs, seed, scratch, hash) })
    } else {
        unsafe { hash_into_scratch(&inputs, seed, scratch, hash) }
    }
}

fn packed_output_len(
    output: &Bound<'_, PyByteArray>,
    items: usize,
    digest_size: usize,
) -> PyResult<usize> {
    let required = items
        .checked_mul(digest_size)
        .ok_or_else(|| PyMemoryError::new_err("XXH3 batch output is too large"))?;
    let provided = unsafe { bytearray_size(output.as_ptr()) };
    if provided < required {
        return Err(PyValueError::new_err(format!(
            "XXH3 batch output requires {required} bytes but the destination has {provided}"
        )));
    }
    Ok(required)
}

fn write_packed_64_at(output: *mut u8, index: usize, hash: u64) {
    let bytes = hash.to_le_bytes();
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), output.add(index * 8), 8) };
}

fn write_packed_128_at(output: *mut u8, index: usize, [low, high]: [u64; 2]) {
    let offset = index * 16;
    let low = low.to_le_bytes();
    let high = high.to_le_bytes();
    unsafe {
        std::ptr::copy_nonoverlapping(low.as_ptr(), output.add(offset), 8);
        std::ptr::copy_nonoverlapping(high.as_ptr(), output.add(offset + 8), 8);
    }
}

trait PackedDigest: Copy + Send + Sync {
    const SIZE: usize;

    fn for_each(inputs: &[&[u8]], seed: u64, callback: impl FnMut(Self));

    fn write_at(output: *mut u8, index: usize, hash: Self);

    #[inline(always)]
    fn collect(inputs: &[&[u8]], seed: u64) -> PyResult<Vec<Self>> {
        let mut hashes = batch_results(inputs.len(), BATCH_TOO_LARGE)?;
        Self::for_each(inputs, seed, |hash| hashes.push(hash));
        Ok(hashes)
    }

    #[inline(always)]
    fn write_direct(
        output: &Bound<'_, PyByteArray>,
        inputs: &[&[u8]],
        seed: u64,
    ) -> PyResult<usize> {
        with_bytearray(output, || {
            let written = packed_output_len(output, inputs.len(), Self::SIZE)?;
            let output = unsafe { bytearray_data(output.as_ptr()) };
            let mut index = 0;
            Self::for_each(inputs, seed, |hash| {
                Self::write_at(output, index, hash);
                index += 1;
            });
            debug_assert_eq!(index, inputs.len());
            Ok(written)
        })
    }

    #[inline(always)]
    fn write_results(output: &Bound<'_, PyByteArray>, hashes: &[Self]) -> PyResult<usize> {
        with_bytearray(output, || {
            let written = packed_output_len(output, hashes.len(), Self::SIZE)?;
            let output = unsafe { bytearray_data(output.as_ptr()) };
            for (index, hash) in hashes.iter().copied().enumerate() {
                Self::write_at(output, index, hash);
            }
            Ok(written)
        })
    }
}

impl PackedDigest for u64 {
    const SIZE: usize = 8;

    #[inline(always)]
    fn for_each(inputs: &[&[u8]], seed: u64, callback: impl FnMut(Self)) {
        xxh3_64_batch_for_each(inputs, seed, callback);
    }

    #[inline(always)]
    fn write_at(output: *mut u8, index: usize, hash: Self) {
        write_packed_64_at(output, index, hash);
    }
}

impl PackedDigest for [u64; 2] {
    const SIZE: usize = 16;

    #[inline(always)]
    fn for_each(inputs: &[&[u8]], seed: u64, callback: impl FnMut(Self)) {
        xxh3_128_batch_for_each(inputs, seed, callback);
    }

    #[inline(always)]
    fn write_at(output: *mut u8, index: usize, hash: Self) {
        write_packed_128_at(output, index, hash);
    }
}

fn int_from_u128<'py>(py: Python<'py>, value: &u128) -> PyResult<Bound<'py, PyInt>> {
    // Older CPython converts from a byte buffer. Pass the staged integer's
    // storage directly instead of copying it into another 16-byte temporary.
    // PyO3 uses the faster integer-writer API on CPython 3.14 and newer.
    #[cfg(Py_3_14)]
    {
        Ok(PyInt::new(py, *value))
    }
    #[cfg(not(Py_3_14))]
    unsafe {
        #[cfg(Py_3_13)]
        let result = ffi::PyLong_FromNativeBytes(
            std::ptr::from_ref(value).cast(),
            std::mem::size_of::<u128>(),
            ffi::Py_ASNATIVEBYTES_NATIVE_ENDIAN | ffi::Py_ASNATIVEBYTES_UNSIGNED_BUFFER,
        );
        #[cfg(not(Py_3_13))]
        let result = ffi::_PyLong_FromByteArray(
            std::ptr::from_ref(value).cast(),
            std::mem::size_of::<u128>(),
            cfg!(target_endian = "little").into(),
            0,
        );
        Ok(Bound::from_owned_ptr_or_err(py, result)?.cast_into_unchecked())
    }
}

pub(super) fn xxh3_64_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let mut scratch = [0; STACK_BATCH_RESULTS];
    // Both batch iterators emit exactly one hash for every input, so this
    // callback initializes the entire destination supplied by `batch_hashes`.
    let hashes = unsafe {
        batch_hashes(py, items, seed, &mut scratch, |inputs, seed, hashes| {
            let mut hashes = hashes.iter_mut();
            xxh3_64_batch_for_each(inputs, seed, |hash| {
                hashes.next().expect("hash count is exact").write(hash);
            });
        })
    }?;
    let hashes = hashes.as_ref();
    list_from_fn(py, hashes.len(), |index| Ok(PyInt::new(py, hashes[index])))
}

pub(super) fn xxh3_128_batch<'py>(
    py: Python<'py>,
    items: &Bound<'py, PyList>,
    seed: u64,
) -> PyResult<Bound<'py, PyList>> {
    let mut scratch = [0_u128; STACK_BATCH_RESULTS];
    // Both batch iterators emit exactly one hash for every input, so this
    // callback initializes the entire destination supplied by `batch_hashes`.
    let hashes = unsafe {
        batch_hashes(py, items, seed, &mut scratch, |inputs, seed, hashes| {
            let mut hashes = hashes.iter_mut();
            xxh3_128_batch_for_each(inputs, seed, |[low, high]| {
                hashes
                    .next()
                    .expect("hash count is exact")
                    .write((u128::from(high) << 64) | u128::from(low));
            });
        })
    }?;
    let hashes = hashes.as_ref();
    list_from_fn(py, hashes.len(), |index| int_from_u128(py, &hashes[index]))
}

#[inline(always)]
fn packed_batch_into<D: PackedDigest>(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    #[cfg(not(Py_GIL_DISABLED))]
    if let Some(exact) = ExactBytesList::checked(items) {
        if exact.len() <= PACKED_STACK_INPUTS && exact.total < XXH3_DETACH_THRESHOLD {
            return exact
                .with_stack::<PACKED_STACK_INPUTS, _>(|inputs| {
                    D::write_direct(output, inputs, seed)
                })
                .expect("stack exact-byte conditions were checked");
        }
        if exact.total < XXH3_DETACH_THRESHOLD {
            let inputs = exact.borrow()?;
            return D::write_direct(output, &inputs, seed);
        }

        let retained = exact.retain()?;
        with_bytearray(output, || {
            packed_output_len(output, retained.len(), D::SIZE)
        })?;
        let inputs = borrow_retained(&retained)?;
        let hashes = py.detach(|| D::collect(&inputs, seed))?;
        return D::write_results(output, &hashes);
    }

    let items = list_items(items)?;
    with_bytearray(output, || packed_output_len(output, items.len(), D::SIZE))?;
    let parsed = parse_batch(&items)?;
    let detach = batch_detach_safe(&parsed);
    let direct = direct_output_safe(&parsed, output, detach);
    let inputs = borrow_batch(&parsed)?;
    if direct {
        return D::write_direct(output, &inputs, seed);
    }
    let hashes = if detach {
        py.detach(|| D::collect(&inputs, seed))?
    } else {
        D::collect(&inputs, seed)?
    };
    drop(inputs);
    drop(parsed);
    D::write_results(output, &hashes)
}

pub(super) fn xxh3_64_batch_into(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    packed_batch_into::<u64>(py, items, output, seed)
}

pub(super) fn xxh3_128_batch_into(
    py: Python<'_>,
    items: &Bound<'_, PyList>,
    output: &Bound<'_, PyByteArray>,
    seed: u64,
) -> PyResult<usize> {
    packed_batch_into::<[u64; 2]>(py, items, output, seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(Py_GIL_DISABLED))]
    #[test]
    fn gil_batch_retains_exact_bytearrays() {
        Python::initialize();
        Python::attach(|py| {
            let items = [PyByteArray::new(py, b"mutable").into_any()];
            let parsed = parse_batch(&items).unwrap();
            assert!(matches!(parsed.first(), Some(BytesLike::ByteArray(_))));
        });
    }

    #[test]
    fn staged_u128_conversion_preserves_unsigned_boundaries() {
        Python::initialize();
        Python::attach(|py| {
            for value in [
                0,
                1,
                u128::from(u64::MAX),
                1 << 64,
                (1 << 127) - 1,
                1 << 127,
                u128::MAX,
            ] {
                let result = int_from_u128(py, &value).unwrap();
                assert_eq!(result.extract::<u128>().unwrap(), value);
            }
        });
    }
}
