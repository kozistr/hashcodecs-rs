use crate::xxhash::{PreparedXxh3, xxh3_64, xxh3_128};

#[test]
fn match_one_shot() {
    let input = (0..=2048)
        .map(|index| (index as u8).wrapping_mul(73).wrapping_add(29))
        .collect::<Vec<_>>();

    for seed in [0, 1, 0x0123_4567_89ab_cdef, u64::MAX] {
        let prepared = PreparedXxh3::new(seed);

        for length in [0, 16, 17, 32, 64, 128, 129, 240, 241, 1024, 2048] {
            assert_eq!(
                prepared.hash_64(&input[..length]),
                xxh3_64(&input[..length], seed)
            );
            assert_eq!(
                prepared.hash_128(&input[..length]),
                xxh3_128(&input[..length], seed)
            );
        }
    }
}

#[test]
fn preserve_seed() {
    assert_eq!(
        format!("{:?}", PreparedXxh3::new(42)),
        "PreparedXxh3 { seed: 42, .. }"
    );
    assert_eq!(
        PreparedXxh3::default().hash_64(b"default"),
        xxh3_64(b"default", 0)
    );
    assert_eq!(
        PreparedXxh3::default().hash_128(b"default"),
        xxh3_128(b"default", 0)
    );
    let prepared = PreparedXxh3::new(42);
    let cloned = prepared.clone();
    let input = vec![0xa5; 2048];
    assert_eq!(cloned.hash_64(&input), prepared.hash_64(&input));
    assert_eq!(cloned.hash_128(&input), prepared.hash_128(&input));
}
