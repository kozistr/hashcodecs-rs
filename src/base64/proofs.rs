use super::decode::{decode_eight_ptr, decode_quad_ptr, decode_unpadded_tail_ptr};
use super::encode::encode_scalar_ptr;
use super::{STANDARD_DECODE, encoded_len};

#[kani::proof]
#[kani::unwind(13)]
fn scalar_encoder_stays_within_the_exact_output_prefix() {
    let input: [u8; 8] = kani::any();
    let length: usize = kani::any();
    let urlsafe: bool = kani::any();
    kani::assume(length <= input.len());
    let required = encoded_len(length);
    let mut output = [0xa5_u8; 12];

    unsafe { encode_scalar_ptr(&input[..length], output.as_mut_ptr(), urlsafe) };

    for byte in &output[required..] {
        assert_eq!(*byte, 0xa5);
    }
}

#[kani::proof]
fn scalar_decoders_stay_within_exact_destinations() {
    let quad: [u8; 4] = kani::any();
    let padding: usize = kani::any();
    kani::assume(padding <= 2);

    let mut quad_output = [0xa5_u8; 3];
    let result =
        unsafe { decode_quad_ptr(&quad, quad_output.as_mut_ptr(), padding, &STANDARD_DECODE) };
    if result.is_ok() {
        if padding >= 1 {
            assert_eq!(quad_output[2], 0xa5);
        }
        if padding == 2 {
            assert_eq!(quad_output[1], 0xa5);
        }
    }

    let octet: [u8; 8] = kani::any();
    let mut octet_output = [0_u8; 6];
    let _ = unsafe { decode_eight_ptr(&octet, octet_output.as_mut_ptr(), &STANDARD_DECODE) };

    let tail: [u8; 3] = kani::any();
    let tail_length: usize = kani::any();
    kani::assume(matches!(tail_length, 2 | 3));

    let mut tail_output = [0xa5_u8; 2];
    let result = unsafe {
        decode_unpadded_tail_ptr(
            &tail[..tail_length],
            tail_output.as_mut_ptr(),
            &STANDARD_DECODE,
        )
    };
    if result.is_ok() && tail_length == 2 {
        assert_eq!(tail_output[1], 0xa5);
    }
}
