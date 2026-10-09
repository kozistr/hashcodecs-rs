#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
use super::long_inputs::{LongBatch, LongRun};
use super::long_inputs::{LongEngine, LongInput, Secret, finalize_long_64, finalize_long_128};
use super::one_shot::{xxh3_64, xxh3_128};

macro_rules! emit_long_group {
    ($name:ident, $size:literal, $(($acc:ident, $index:literal)),+ $(,)?) => {
        #[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
        #[inline(always)]
        fn $name<T, F, O>(
            secret: &Secret,
            inputs: LongBatch<'_, $size>,
            accumulators: [[u64; 8]; $size],
            finalize: F,
            output: &mut O,
        ) where
            F: Copy + Fn(usize, &Secret, [u64; 8]) -> T,
            O: FnMut(T),
        {
            let [$($acc),+] = accumulators;
            $(output(finalize(inputs.input($index).len(), secret, $acc));)+
        }
    };
}

emit_long_group!(emit_long_group2, 2, (acc0, 0), (acc1, 1));
emit_long_group!(emit_long_group3, 3, (acc0, 0), (acc1, 1), (acc2, 2));
emit_long_group!(
    emit_long_group4,
    4,
    (acc0, 0),
    (acc1, 1),
    (acc2, 2),
    (acc3, 3),
);

/// Runs one batch loop for vector outputs and callback outputs.
#[inline(always)]
fn hash_each_input<T, S, F, O>(inputs: &[&[u8]], seed: u64, short: S, finalize: F, mut output: O)
where
    S: Copy + Fn(&[u8], u64) -> T,
    F: Copy + Fn(usize, &Secret, [u64; 8]) -> T,
    O: FnMut(T),
{
    let mut index = 0;

    while index < inputs.len() && inputs[index].len() <= 240 {
        output(short(inputs[index], seed));
        index += 1;
    }

    if index == inputs.len() {
        return;
    }

    let engine = LongEngine::cached();
    let derived_secret = engine.derive_secret(seed);
    hash_each_input_with_secret(
        &inputs[index..],
        seed,
        short,
        finalize,
        engine,
        LongEngine::secret(derived_secret.as_ref()),
        &mut output,
    );
}

#[inline(always)]
pub(super) fn hash_each_input_with_secret<T, S, F, O>(
    inputs: &[&[u8]],
    seed: u64,
    short: S,
    finalize: F,
    engine: &LongEngine,
    secret: &Secret,
    mut output: O,
) where
    S: Copy + Fn(&[u8], u64) -> T,
    F: Copy + Fn(usize, &Secret, [u64; 8]) -> T,
    O: FnMut(T),
{
    let mut index = 0;

    while index < inputs.len() && inputs[index].len() <= 240 {
        output(short(inputs[index], seed));
        index += 1;
    }

    if index == inputs.len() {
        return;
    }

    // Only x86 has an accelerated batch kernel. Keep its scheduling path out
    // of production builds on architectures that process inputs individually.
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if engine.has_batch_kernel() {
        hash_input_runs(
            &inputs[index..],
            seed,
            short,
            finalize,
            engine,
            secret,
            output,
        );

        return;
    }

    while index < inputs.len() {
        if let Some(input) = LongInput::new(inputs[index]) {
            output(engine.hash(input, secret, finalize));
        } else {
            output(short(inputs[index], seed));
        }

        index += 1;
    }
}

#[cfg(any(test, target_arch = "x86", target_arch = "x86_64"))]
#[inline(always)]
fn hash_input_runs<T, S, F, O>(
    inputs: &[&[u8]],
    seed: u64,
    short: S,
    finalize: F,
    engine: &LongEngine,
    secret: &Secret,
    mut output: O,
) where
    S: Copy + Fn(&[u8], u64) -> T,
    F: Copy + Fn(usize, &Secret, [u64; 8]) -> T,
    O: FnMut(T),
{
    let mut index = 0;

    while index < inputs.len() {
        let Some(run) = LongRun::new(&inputs[index..]) else {
            output(short(inputs[index], seed));
            index += 1;

            continue;
        };

        if run.len() == 4 {
            let group = run.batch4(0);
            let accumulators = engine.accumulate_batch4(group, secret);
            emit_long_group4(secret, group, accumulators, finalize, &mut output);
        } else if run.len() == 3 {
            let group = run.batch3(0);
            let accumulators = engine.accumulate_batch3(group, secret);
            emit_long_group3(secret, group, accumulators, finalize, &mut output);
        } else if run.len() == 2 {
            let group = run.batch2(0);
            let accumulators = engine.accumulate_batch2(group, secret);
            emit_long_group2(secret, group, accumulators, finalize, &mut output);
        } else {
            let input = run.input(0);
            output(engine.hash(input, secret, finalize));
        }

        index += run.len();
    }
}

/// Computes canonical XXH3 64-bit hashes for a batch without copying inputs.
///
/// The result order matches the input order. The function shares seed setup across the batch.
/// The AVX2 kernel can process two to four adjacent long inputs with equal stripe counts at one time.
///
/// # Arguments
///
/// * `inputs` - Contains the borrowed byte slices to hash in order.
/// * `seed` - Specifies one initial unsigned 64-bit seed for all inputs.
///
/// # Returns
///
/// The function returns one canonical 64-bit hash for each input.
///
/// # Examples
///
///     use hashcodecs::xxhash::{xxh3_64, xxh3_64_batch};
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     assert_eq!(
///         xxh3_64_batch(inputs, 7),
///         inputs.iter().map(|input| xxh3_64(input, 7)).collect::<Vec<_>>(),
///     );
///
#[inline]
pub fn xxh3_64_batch(inputs: &[&[u8]], seed: u64) -> Vec<u64> {
    let mut hashes = Vec::with_capacity(inputs.len());

    hash_each_input(inputs, seed, xxh3_64, finalize_long_64, |hash| {
        hashes.push(hash)
    });

    hashes
}

/// Computes canonical XXH3 64-bit hashes and sends each result to a callback.
///
/// This function does not allocate a result vector. It calls `output` once for each input, in input order.
/// The function shares seed setup and eligible long-input processing across the batch.
///
/// # Examples
///
///     use hashcodecs::xxhash::xxh3_64_batch_for_each;
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     let mut hashes = [0; 2];
///     let mut index = 0;
///     xxh3_64_batch_for_each(inputs, 7, |hash| {
///         hashes[index] = hash;
///         index += 1;
///     });
///     assert_eq!(index, inputs.len());
///
#[inline]
pub fn xxh3_64_batch_for_each(inputs: &[&[u8]], seed: u64, output: impl FnMut(u64)) {
    hash_each_input(inputs, seed, xxh3_64, finalize_long_64, output);
}

/// Computes canonical XXH3 128-bit hashes for a batch without copying inputs.
///
/// The result order matches the input order. The function shares seed setup across the batch.
/// The AVX2 kernel can process two to four adjacent long inputs with equal stripe counts at one time.
///
/// # Arguments
///
/// * `inputs` - Contains the borrowed byte slices to hash in order.
/// * `seed` - Specifies one initial unsigned 64-bit seed for all inputs.
///
/// # Returns
///
/// The function returns one `[low64, high64]` word pair for each input.
/// Each pair follows the contract for [`crate::xxhash::xxh3_128`].
///
/// # Examples
///
///     use hashcodecs::xxhash::{xxh3_128, xxh3_128_batch};
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     assert_eq!(
///         xxh3_128_batch(inputs, 7),
///         inputs.iter().map(|input| xxh3_128(input, 7)).collect::<Vec<_>>(),
///     );
///
#[inline]
pub fn xxh3_128_batch(inputs: &[&[u8]], seed: u64) -> Vec<[u64; 2]> {
    let mut hashes = Vec::with_capacity(inputs.len());

    hash_each_input(inputs, seed, xxh3_128, finalize_long_128, |hash| {
        hashes.push(hash)
    });

    hashes
}

/// Computes canonical XXH3 128-bit hashes and sends each result to a callback.
///
/// This function does not allocate a result vector.
/// It calls `output` for each input, in input order, with a `[low64, high64]` word pair.
///
/// # Examples
///
///     use hashcodecs::xxhash::xxh3_128_batch_for_each;
///
///     let inputs: &[&[u8]] = &[b"one", b"two"];
///     let mut hashes = [[0; 2]; 2];
///     let mut index = 0;
///     xxh3_128_batch_for_each(inputs, 7, |hash| {
///         hashes[index] = hash;
///         index += 1;
///     });
///     assert_eq!(index, inputs.len());
///
#[inline]
pub fn xxh3_128_batch_for_each(inputs: &[&[u8]], seed: u64, output: impl FnMut([u64; 2])) {
    hash_each_input(inputs, seed, xxh3_128, finalize_long_128, output);
}

#[cfg(test)]
mod tests;
