use super::*;
use crate::backend::Capabilities;

fn hashes_64_with_engine(inputs: &[&[u8]], engine: &LongEngine) -> Vec<u64> {
    let mut hashes = Vec::new();

    let derived = engine.derive_secret(17);
    hash_each_input_with_secret(
        inputs,
        17,
        xxh3_64,
        finalize_long_64,
        engine,
        LongEngine::secret(derived.as_ref()),
        &mut |hash| hashes.push(hash),
    );

    hashes
}

fn hashes_128_with_engine(inputs: &[&[u8]], engine: &LongEngine) -> Vec<[u64; 2]> {
    let mut hashes = Vec::new();

    let derived = engine.derive_secret(17);
    hash_each_input_with_secret(
        inputs,
        17,
        xxh3_128,
        finalize_long_128,
        engine,
        LongEngine::secret(derived.as_ref()),
        &mut |hash| hashes.push(hash),
    );

    hashes
}

#[test]
fn compare_engines() {
    let owned = [
        300, 300, 300, 300, 17, 301, 301, 301, 17, 302, 302, 17, 1024,
    ]
    .map(|length| vec![length as u8; length]);

    let refs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let scalar = LongEngine::new_with_capabilities(Capabilities::from_features(&[]));
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
fn match_grouped_runs() {
    let owned = [
        257, 258, 259, 260, 17, 1025, 1026, 1088, 17, 1089, 1090, 17, 2048,
    ]
    .map(|length| vec![length as u8; length]);
    let inputs = owned.each_ref().map(Vec::as_slice);

    for engine in [
        LongEngine::new_with_capabilities(Capabilities::from_features(&[])),
        LongEngine::new(),
    ] {
        let derived = engine.derive_secret(17);
        let secret = LongEngine::secret(derived.as_ref());
        let mut hashes_64 = Vec::new();
        hash_input_runs(
            &inputs,
            17,
            xxh3_64,
            finalize_long_64,
            &engine,
            secret,
            |hash| hashes_64.push(hash),
        );
        assert_eq!(hashes_64, inputs.map(|input| xxh3_64(input, 17)));
        let mut hashes_128 = Vec::new();
        hash_input_runs(
            &inputs,
            17,
            xxh3_128,
            finalize_long_128,
            &engine,
            secret,
            |hash| hashes_128.push(hash),
        );
        assert_eq!(hashes_128, inputs.map(|input| xxh3_128(input, 17)));
    }
}

#[test]
fn limit_long_runs() {
    let owned = [257, 258, 260, 17, 261, 320, 321].map(|length| vec![0; length]);
    let refs = owned.each_ref().map(Vec::as_slice);

    assert_eq!(LongRun::new(&refs).unwrap().len(), 3);
    assert!(LongRun::new(&refs[3..]).is_none());
    assert_eq!(LongRun::new(&refs[4..]).unwrap().len(), 2);
    assert_eq!(LongRun::new(&refs[6..]).unwrap().len(), 1);

    let compatible = [300; 8].map(|length| vec![0; length]);
    let compatible_refs = compatible.each_ref().map(Vec::as_slice);
    assert_eq!(LongRun::new(&compatible_refs).unwrap().len(), 4);
}

#[test]
fn emit_scalar_lanes() {
    let owned = [300, 300].map(|length| vec![length as u8; length]);
    let refs = owned.each_ref().map(Vec::as_slice);
    let run = LongRun::new(&refs).unwrap();
    let inputs = run.batch2(0);
    let engine = LongEngine::new_with_capabilities(Capabilities::from_features(&[]));
    let derived = engine.derive_secret(17);

    let mut actual = Vec::new();

    emit_long_group2(
        LongEngine::secret(derived.as_ref()),
        inputs,
        engine.accumulate_batch2(inputs, LongEngine::secret(derived.as_ref())),
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

    let mut actual = Vec::new();
    emit_long_group2(
        LongEngine::secret(derived.as_ref()),
        inputs,
        engine.accumulate_batch2(inputs, LongEngine::secret(derived.as_ref())),
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
        LongEngine::secret(derived.as_ref()),
        group3,
        engine.accumulate_batch3(group3, LongEngine::secret(derived.as_ref())),
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
        LongEngine::secret(derived.as_ref()),
        group4,
        engine.accumulate_batch4(group4, LongEngine::secret(derived.as_ref())),
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
        LongEngine::secret(derived.as_ref()),
        group3,
        engine.accumulate_batch3(group3, LongEngine::secret(derived.as_ref())),
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
        LongEngine::secret(derived.as_ref()),
        group4,
        engine.accumulate_batch4(group4, LongEngine::secret(derived.as_ref())),
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
