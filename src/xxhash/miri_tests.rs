use super::*;

#[test]
fn every_length_class_and_batch_are_defined() {
    const LENGTHS: &[usize] = &[
        0, 1, 3, 4, 8, 9, 16, 17, 32, 33, 64, 65, 96, 97, 128, 129, 160, 191, 224, 239, 240, 241,
        1023, 1024, 1025, 2049,
    ];
    for &length in LENGTHS {
        let input = (0..length)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
            .collect::<Vec<_>>();
        for &seed in &[0, 1, u64::MAX] {
            let _ = xxh3_64(&input, seed);
            let _ = xxh3_128(&input, seed);
        }
    }

    let owned = (0..4).map(|item| vec![item; 2049]).collect::<Vec<_>>();
    let inputs = owned.iter().map(Vec::as_slice).collect::<Vec<_>>();
    assert_eq!(
        xxh3_64_batch(&inputs, 42),
        inputs
            .iter()
            .map(|input| xxh3_64(input, 42))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        xxh3_128_batch(&inputs, 42),
        inputs
            .iter()
            .map(|input| xxh3_128(input, 42))
            .collect::<Vec<_>>()
    );
}
