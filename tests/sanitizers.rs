use hashcodecs::{
    base64::{
        b64decode_into, b64decode_urlsafe_into, b64decoded_len, b64encode_into,
        b64encode_urlsafe_into, b64encoded_len,
    },
    murmur3::{
        Murmur3X64Hasher128, Murmur3X86Hasher32, Murmur3X86Hasher128, murmur3_x64_128,
        murmur3_x86_32, murmur3_x86_128,
    },
    xxhash::{xxh3_64, xxh3_64_batch, xxh3_128, xxh3_128_batch},
};

fn payload(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

fn check_base64(input: &[u8], urlsafe: bool) {
    let encoded_capacity = b64encoded_len(input.len()).expect("test payload length must fit");
    let mut encoded = vec![0xa5; encoded_capacity];
    let encoded_len = if urlsafe {
        b64encode_urlsafe_into(input, &mut encoded)
    } else {
        b64encode_into(input, &mut encoded)
    }
    .expect("exact-size encode buffer must succeed");
    assert_eq!(encoded_len, encoded.len());

    let decoded_len = b64decoded_len(&encoded).expect("encoded data must be valid");
    let mut decoded = vec![0xa5; decoded_len];
    let written = if urlsafe {
        b64decode_urlsafe_into(&encoded, &mut decoded)
    } else {
        b64decode_into(&encoded, &mut decoded)
    }
    .expect("exact-size decode buffer must succeed");
    assert_eq!(written, input.len());
    assert_eq!(decoded, input);
}

fn check_murmur(input: &[u8], seed: u32) {
    let mut x86_32 = Murmur3X86Hasher32::new(seed);
    let mut x86_128 = Murmur3X86Hasher128::new(seed);
    let mut x64_128 = Murmur3X64Hasher128::new(seed);
    for chunk in input.chunks(7) {
        x86_32.update(chunk);
        x86_128.update(chunk);
        x64_128.update(chunk);
    }
    assert_eq!(x86_32.digest(), murmur3_x86_32(input, seed));
    assert_eq!(x86_128.digest(), murmur3_x86_128(input, seed));
    assert_eq!(x64_128.digest(), murmur3_x64_128(input, seed));
}

fn check_xxhash(inputs: &[Vec<u8>], seed: u64) {
    let slices: Vec<&[u8]> = inputs.iter().map(Vec::as_slice).collect();
    let expected_64: Vec<u64> = slices.iter().map(|input| xxh3_64(input, seed)).collect();
    let expected_128: Vec<[u64; 2]> = slices.iter().map(|input| xxh3_128(input, seed)).collect();
    assert_eq!(xxh3_64_batch(&slices, seed), expected_64);
    assert_eq!(xxh3_128_batch(&slices, seed), expected_128);
}

fn main() {
    let lengths = [
        0, 1, 2, 3, 4, 7, 8, 11, 12, 15, 16, 17, 31, 32, 47, 48, 63, 64, 95, 96, 127, 128, 129,
        191, 192, 239, 240, 241, 255, 256, 511, 512, 1024, 4096,
    ];
    let inputs: Vec<Vec<u8>> = lengths.into_iter().map(payload).collect();
    for input in &inputs {
        check_base64(input, false);
        check_base64(input, true);
        check_murmur(input, 0x9747_b28c);
    }
    check_xxhash(&inputs, 0x9e37_79b1_85eb_ca87);

    let mut equal_long_inputs: Vec<Vec<u8>> = (0..4).map(|_| payload(4096)).collect();
    for (index, input) in equal_long_inputs.iter_mut().enumerate() {
        input[0] = input[0].wrapping_add(index as u8);
        input[4095] ^= (index as u8).wrapping_mul(0x5b);
    }
    for width in 2..=4 {
        check_xxhash(&equal_long_inputs[..width], 0x9e37_79b1_85eb_ca87);
    }
}
