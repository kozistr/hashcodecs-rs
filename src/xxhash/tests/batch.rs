use crate::xxhash::{
    PreparedXxh3, xxh3_64, xxh3_64_batch, xxh3_64_batch_for_each, xxh3_128, xxh3_128_batch,
    xxh3_128_batch_for_each,
};

#[test]
fn skip_empty_visitors() {
    assert!(xxh3_64_batch(&[], 42).is_empty());
    assert!(xxh3_128_batch(&[], 42).is_empty());
    xxh3_64_batch_for_each(&[], 42, |_| panic!("empty batch produced a hash"));
    xxh3_128_batch_for_each(&[], 42, |_| panic!("empty batch produced a hash"));

    let prepared = PreparedXxh3::new(42);
    assert!(prepared.hash_64_batch(&[]).is_empty());
    assert!(prepared.hash_128_batch(&[]).is_empty());
    prepared.hash_64_batch_for_each(&[], |_| panic!("empty batch produced a hash"));
    prepared.hash_128_batch_for_each(&[], |_| panic!("empty batch produced a hash"));
}

#[test]
fn match_one_shot() {
    let values: [&[u8]; 3] = [b"", b"hello", b"xxhash"];
    assert_eq!(xxh3_64_batch(&values, 42), values.map(|v| xxh3_64(v, 42)));
    assert_eq!(xxh3_128_batch(&values, 42), values.map(|v| xxh3_128(v, 42)));

    let mut hashes_64 = [0; 3];
    let mut hashes_128 = [[0; 2]; 3];
    let mut index = 0;
    xxh3_64_batch_for_each(&values, 42, |hash| {
        hashes_64[index] = hash;
        index += 1;
    });
    assert_eq!(index, values.len());
    let mut index = 0;
    xxh3_128_batch_for_each(&values, 42, |hash| {
        hashes_128[index] = hash;
        index += 1;
    });
    assert_eq!(index, values.len());
    assert_eq!(hashes_64, values.map(|value| xxh3_64(value, 42)));
    assert_eq!(hashes_128, values.map(|value| xxh3_128(value, 42)));

    let mixed_owned = [17, 129, 241, 300].map(|length| {
        (0..length)
            .map(|index| (index as u8).wrapping_mul(19).wrapping_add(7))
            .collect::<Vec<_>>()
    });
    let mixed = mixed_owned.each_ref().map(Vec::as_slice);
    assert_eq!(xxh3_64_batch(&mixed, 42), mixed.map(|v| xxh3_64(v, 42)));
    assert_eq!(xxh3_128_batch(&mixed, 42), mixed.map(|v| xxh3_128(v, 42)));

    for item_count in 2..=8 {
        let owned = (0..item_count)
            .map(|item| {
                (0..4161)
                    .map(|index| (index as u8).wrapping_mul(31).wrapping_add(item))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert_eq!(
            xxh3_64_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| xxhash_rust::xxh3::xxh3_64_with_seed(input, 0x1234_5678))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            xxh3_128_batch(&inputs, 0x1234_5678),
            inputs
                .iter()
                .map(|input| {
                    let hash = xxhash_rust::xxh3::xxh3_128_with_seed(input, 0x1234_5678);
                    [hash as u64, (hash >> 64) as u64]
                })
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn preserve_stripe_order() {
    let owned = [257, 258, 259, 260, 17, 1025, 1026, 1088, 1089].map(|length| {
        (0..length)
            .map(|index| (index as u8).wrapping_mul(43).wrapping_add(length as u8))
            .collect::<Vec<_>>()
    });
    let inputs = owned.each_ref().map(Vec::as_slice);

    for seed in [0, 0x0123_4567_89ab_cdef] {
        assert_eq!(
            xxh3_64_batch(&inputs, seed),
            inputs.map(|input| xxh3_64(input, seed))
        );
        assert_eq!(
            xxh3_128_batch(&inputs, seed),
            inputs.map(|input| xxh3_128(input, seed))
        );
    }
}

#[test]
fn match_prepared_batches() {
    let owned = [17, 241, 257, 258, 259, 260, 1024, 1025].map(|length| vec![length as u8; length]);
    let inputs = owned.each_ref().map(Vec::as_slice);

    for seed in [0, 0x0123_4567_89ab_cdef] {
        let prepared = PreparedXxh3::new(seed);
        assert_eq!(
            prepared.hash_64_batch(&inputs),
            inputs.map(|input| xxh3_64(input, seed))
        );
        assert_eq!(
            prepared.hash_128_batch(&inputs),
            inputs.map(|input| xxh3_128(input, seed))
        );

        let mut hashes_64 = Vec::new();
        prepared.hash_64_batch_for_each(&inputs, |hash| hashes_64.push(hash));
        assert_eq!(hashes_64, inputs.map(|input| xxh3_64(input, seed)));

        let mut hashes_128 = Vec::new();
        prepared.hash_128_batch_for_each(&inputs, |hash| hashes_128.push(hash));
        assert_eq!(hashes_128, inputs.map(|input| xxh3_128(input, seed)));
    }
}
