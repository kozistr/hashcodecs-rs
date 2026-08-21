use pyo3::prelude::*;

use super::DETACH_THRESHOLD;
use super::buffer::{BytesLike, bytes_like};
use crate::{xxh3_64 as xxh3_64_hash, xxh3_128 as xxh3_128_hash};

mod batch;
mod single;

pub use batch::{xxh3_64_batch, xxh3_128_batch};
pub use single::{xxh3_64, xxh3_128};

fn hash64(py: Python<'_>, value: &Bound<'_, PyAny>, seed: u64) -> PyResult<u64> {
    let input = bytes_like(py, value, "s")?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    Ok(unsafe {
        input.with_bytes(|bytes| {
            if detach {
                py.detach(|| xxh3_64_hash(bytes, seed))
            } else {
                xxh3_64_hash(bytes, seed)
            }
        })
    })
}

fn hash128(py: Python<'_>, value: &Bound<'_, PyAny>, seed: u64) -> PyResult<u128> {
    let input = bytes_like(py, value, "s")?;
    let detach = input.detach_safe() && input.len() >= DETACH_THRESHOLD;
    Ok(unsafe {
        input.with_bytes(|bytes| {
            let [low, high] = if detach {
                py.detach(|| xxh3_128_hash(bytes, seed))
            } else {
                xxh3_128_hash(bytes, seed)
            };
            (u128::from(high) << 64) | u128::from(low)
        })
    })
}

fn parse_batch<'a, 'py>(
    py: Python<'py>,
    items: &'a [Bound<'py, PyAny>],
) -> PyResult<Vec<BytesLike<'a, 'py>>> {
    items
        .iter()
        .map(|item| bytes_like(py, item, "items element"))
        .collect()
}

fn batch_detach_safe(inputs: &[BytesLike<'_, '_>]) -> bool {
    let total = inputs
        .iter()
        .fold(0_usize, |total, input| total.saturating_add(input.len()));
    inputs.iter().all(BytesLike::detach_safe) && total >= DETACH_THRESHOLD
}

fn borrow_batch<'a>(inputs: &'a [BytesLike<'_, '_>]) -> Vec<&'a [u8]> {
    inputs
        .iter()
        .map(|input| unsafe { input.as_bytes() })
        .collect()
}
