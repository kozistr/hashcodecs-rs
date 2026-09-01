use std::arch::aarch64::*;

use super::scalar::{is_lenient_symbol, translate as translate_scalar};

#[target_feature(enable = "neon")]
pub(super) unsafe fn symbol_count(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    let [extra0, extra1] = altchars.unwrap_or(*b"AA");
    let mut source = 0;
    let mut symbols = 0;
    while source + 16 <= input.len() {
        let bytes = unsafe { vld1q_u8(input.as_ptr().add(source)) };
        let range = |lower, upper| {
            vandq_u8(
                vcgeq_u8(bytes, vdupq_n_u8(lower)),
                vcleq_u8(bytes, vdupq_n_u8(upper)),
            )
        };
        let valid = vorrq_u8(
            vorrq_u8(range(b'A', b'Z'), range(b'a', b'z')),
            vorrq_u8(
                range(b'0', b'9'),
                vorrq_u8(
                    vorrq_u8(
                        vceqq_u8(bytes, vdupq_n_u8(b'+')),
                        vceqq_u8(bytes, vdupq_n_u8(b'/')),
                    ),
                    vorrq_u8(
                        vceqq_u8(bytes, vdupq_n_u8(extra0)),
                        vceqq_u8(bytes, vdupq_n_u8(extra1)),
                    ),
                ),
            ),
        );
        symbols += vaddvq_u8(vshrq_n_u8::<7>(valid)) as usize;
        source += 16;
    }
    symbols
        + input[source..]
            .iter()
            .filter(|&&byte| is_lenient_symbol(byte, altchars))
            .count()
}

#[target_feature(enable = "neon")]
pub(super) unsafe fn translate(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let mut offset = 0;
    while offset + 16 <= input.len() {
        let bytes = unsafe { vld1q_u8(input.as_ptr().add(offset)) };
        let translated0 = vbslq_u8(
            vceqq_u8(bytes, vdupq_n_u8(source0)),
            vdupq_n_u8(target0),
            bytes,
        );
        let translated1 = vbslq_u8(
            vceqq_u8(bytes, vdupq_n_u8(source1)),
            vdupq_n_u8(target1),
            translated0,
        );
        unsafe { vst1q_u8(input.as_mut_ptr().add(offset), translated1) };
        offset += 16;
    }
    unsafe { translate_scalar(&mut input[offset..], source0, target0, source1, target1) };
}
