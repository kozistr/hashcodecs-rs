use super::hash::{xxh3_64, xxh3_128};
use super::long::{
    LongBatch, LongEngine, LongInput, LongRun, Secret, finalize_long_64, finalize_long_128,
};

macro_rules! emit_long_group {
    ($name:ident, $size:literal, $($acc:ident),+ $(,)?) => {
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
            let length = inputs.input(0).len();
            $(output(finalize(length, secret, $acc));)+
        }
    };
}

emit_long_group!(emit_long_group2, 2, acc0, acc1);
emit_long_group!(emit_long_group3, 3, acc0, acc1, acc2);
emit_long_group!(emit_long_group4, 4, acc0, acc1, acc2, acc3);

/// Shared monomorphized traversal for returned vectors and callback consumers.
#[inline(always)]
fn batch_each<T, S, F, O>(inputs: &[&[u8]], seed: u64, short: S, finalize: F, mut output: O)
where
    S: Copy + Fn(&[u8], u64) -> T,
    F: Copy + Fn(usize, &Secret, [u64; 8]) -> T,
    O: FnMut(T),
{
    batch_each_with_engine(
        inputs,
        seed,
        short,
        finalize,
        &LongEngine::new(),
        &mut output,
    );
}

#[inline(always)]
fn batch_each_with_engine<T, S, F, O>(
    inputs: &[&[u8]],
    seed: u64,
    short: S,
    finalize: F,
    engine: &LongEngine,
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

    let derived_secret = engine.derive_secret(seed);
    let secret = engine.secret(&derived_secret);
    if !engine.has_batch_kernel() {
        while index < inputs.len() {
            if let Some(input) = LongInput::new(inputs[index]) {
                output(engine.hash(input, secret, finalize));
            } else {
                output(short(inputs[index], seed));
            }
            index += 1;
        }
        return;
    }
    while index < inputs.len() {
        let Some(run) = LongRun::new(&inputs[index..]) else {
            output(short(inputs[index], seed));
            index += 1;
            continue;
        };

        let mut run_index = 0;
        while run_index + 4 <= run.len() {
            let group = run.batch4(run_index);
            let accumulators = engine.accumulate_batch4(group, secret);
            emit_long_group4(secret, group, accumulators, finalize, &mut output);
            run_index += 4;
        }
        match run.len() - run_index {
            3 => {
                let group = run.batch3(run_index);
                let accumulators = engine.accumulate_batch3(group, secret);
                emit_long_group3(secret, group, accumulators, finalize, &mut output);
            }
            2 => {
                let group = run.batch2(run_index);
                let accumulators = engine.accumulate_batch2(group, secret);
                emit_long_group2(secret, group, accumulators, finalize, &mut output);
            }
            1 => {
                let input = run.first(run_index);
                output(engine.hash(input, secret, finalize));
            }
            _ => {}
        }
        index += run.len();
    }
}

/// Computes canonical XXH3 64-bit hashes for a batch without copying inputs.
///
/// Results preserve input order. Seed-derived setup is shared by the batch, and
/// contiguous equal-size long runs may be processed two to four at a time when
/// the AVX2 batch kernel is available.
///
/// # Arguments
///
/// * inputs - Borrowed byte slices to hash in order.
/// * seed - The initial unsigned 64-bit seed shared by every input.
///
/// # Returns
///
/// One canonical 64-bit hash per input.
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
    batch_each(inputs, seed, xxh3_64, finalize_long_64, |hash| {
        hashes.push(hash)
    });
    hashes
}

#[cfg(feature = "python")]
#[inline]
pub(crate) fn xxh3_64_batch_each(inputs: &[&[u8]], seed: u64, output: impl FnMut(u64)) {
    batch_each(inputs, seed, xxh3_64, finalize_long_64, output);
}

/// Computes canonical XXH3 128-bit hashes for a batch without copying inputs.
///
/// Results preserve input order. Seed-derived setup is shared by the batch, and
/// contiguous equal-size long runs may be processed two to four at a time when
/// the AVX2 batch kernel is available.
///
/// # Arguments
///
/// * inputs - Borrowed byte slices to hash in order.
/// * seed - The initial unsigned 64-bit seed shared by every input.
///
/// # Returns
///
/// One `[low64, high64]` word pair per input. Each pair follows the same
/// contract as [`crate::xxhash::xxh3_128`].
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
    batch_each(inputs, seed, xxh3_128, finalize_long_128, |hash| {
        hashes.push(hash)
    });
    hashes
}

#[cfg(feature = "python")]
#[inline]
pub(crate) fn xxh3_128_batch_each(inputs: &[&[u8]], seed: u64, output: impl FnMut([u64; 2])) {
    batch_each(inputs, seed, xxh3_128, finalize_long_128, output);
}

#[cfg(all(test, any(target_arch = "x86", target_arch = "x86_64")))]
mod tests {
    use super::*;
    use crate::backend::Capabilities;

    fn hashes_64_with_engine(inputs: &[&[u8]], engine: &LongEngine) -> Vec<u64> {
        let mut hashes = Vec::new();
        batch_each_with_engine(inputs, 17, xxh3_64, finalize_long_64, engine, &mut |hash| {
            hashes.push(hash)
        });
        hashes
    }

    fn hashes_128_with_engine(inputs: &[&[u8]], engine: &LongEngine) -> Vec<[u64; 2]> {
        let mut hashes = Vec::new();
        batch_each_with_engine(
            inputs,
            17,
            xxh3_128,
            finalize_long_128,
            engine,
            &mut |hash| hashes.push(hash),
        );
        hashes
    }

    #[test]
    fn scalar_batch_engine_processes_and_finalizes_inputs_individually() {
        let owned = [
            300, 300, 300, 300, 17, 301, 301, 301, 17, 302, 302, 17, 1024,
        ]
        .map(|length| vec![length as u8; length]);
        let refs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let scalar = LongEngine::new_with_capabilities(Capabilities::for_backends(&[]));
        assert!(!scalar.has_batch_kernel());

        let short = [b"short".as_slice()];
        assert_eq!(
            hashes_64_with_engine(&short, &scalar),
            vec![xxh3_64(short[0], 17)]
        );
        assert_eq!(
            hashes_128_with_engine(&short, &scalar),
            vec![xxh3_128(short[0], 17)]
        );

        assert_eq!(
            hashes_64_with_engine(&refs, &scalar),
            owned
                .iter()
                .map(|input| xxh3_64(input, 17))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            hashes_128_with_engine(&refs, &scalar),
            owned
                .iter()
                .map(|input| xxh3_128(input, 17))
                .collect::<Vec<_>>()
        );

        let native = LongEngine::new();
        assert_eq!(
            hashes_64_with_engine(&refs, &native),
            owned
                .iter()
                .map(|input| xxh3_64(input, 17))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            hashes_128_with_engine(&refs, &native),
            owned
                .iter()
                .map(|input| xxh3_128(input, 17))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn scalar_engine_falls_back_for_batch_groups() {
        let owned = [300, 300].map(|length| vec![length as u8; length]);
        let refs = owned.each_ref().map(Vec::as_slice);
        let run = LongRun::new(&refs).unwrap();
        let inputs = run.batch2(0);
        let engine = LongEngine::new_with_capabilities(Capabilities::for_backends(&[]));
        let derived = engine.derive_secret(17);
        let mut actual = Vec::new();
        emit_long_group2(
            engine.secret(&derived),
            inputs,
            engine.accumulate_batch2(inputs, engine.secret(&derived)),
            finalize_long_64,
            &mut |hash| {
                actual.push(hash);
            },
        );
        assert_eq!(
            actual,
            owned
                .iter()
                .map(|input| xxh3_64(input, 17))
                .collect::<Vec<_>>()
        );

        let engine = LongEngine::new_with_capabilities(Capabilities::for_backends(&[]));
        let derived = engine.derive_secret(17);
        let mut actual = Vec::new();
        emit_long_group2(
            engine.secret(&derived),
            inputs,
            engine.accumulate_batch2(inputs, engine.secret(&derived)),
            finalize_long_128,
            &mut |hash| {
                actual.push(hash);
            },
        );
        assert_eq!(
            actual,
            owned
                .iter()
                .map(|input| xxh3_128(input, 17))
                .collect::<Vec<_>>()
        );

        let owned = [300, 300, 300, 300].map(|length| vec![length as u8; length]);
        let refs = owned.each_ref().map(Vec::as_slice);
        let run = LongRun::new(&refs).unwrap();
        let group3 = run.batch3(0);
        let group4 = run.batch4(0);
        let mut actual = Vec::new();
        emit_long_group3(
            engine.secret(&derived),
            group3,
            engine.accumulate_batch3(group3, engine.secret(&derived)),
            finalize_long_64,
            &mut |hash| actual.push(hash),
        );
        assert_eq!(
            actual,
            owned[..3]
                .iter()
                .map(|input| xxh3_64(input, 17))
                .collect::<Vec<_>>()
        );

        let mut actual = Vec::new();
        emit_long_group4(
            engine.secret(&derived),
            group4,
            engine.accumulate_batch4(group4, engine.secret(&derived)),
            finalize_long_64,
            &mut |hash| actual.push(hash),
        );
        assert_eq!(
            actual,
            owned
                .iter()
                .map(|input| xxh3_64(input, 17))
                .collect::<Vec<_>>()
        );

        let mut actual = Vec::new();
        emit_long_group3(
            engine.secret(&derived),
            group3,
            engine.accumulate_batch3(group3, engine.secret(&derived)),
            finalize_long_128,
            &mut |hash| actual.push(hash),
        );
        assert_eq!(
            actual,
            owned[..3]
                .iter()
                .map(|input| xxh3_128(input, 17))
                .collect::<Vec<_>>()
        );

        let mut actual = Vec::new();
        emit_long_group4(
            engine.secret(&derived),
            group4,
            engine.accumulate_batch4(group4, engine.secret(&derived)),
            finalize_long_128,
            &mut |hash| actual.push(hash),
        );
        assert_eq!(
            actual,
            owned
                .iter()
                .map(|input| xxh3_128(input, 17))
                .collect::<Vec<_>>()
        );
    }
}
