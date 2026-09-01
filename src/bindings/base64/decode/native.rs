mod advanced;
mod lenient;
mod strict;

use advanced::decode_advanced_strict_into;
pub(super) use advanced::{decode_advanced, decode_advanced_into};
pub(super) use lenient::{
    lenient_continues_after_padding, try_decode_lenient, try_decode_lenient_into,
};
pub(super) use strict::{
    decode_strict, decode_strict_into, decode_strict_into_with_altchars,
    decode_strict_with_altchars, decode_unpadded_into_with_altchars, decode_unpadded_with_altchars,
    translate_altchars, try_decode_strict, try_decode_urlsafe_315, try_decode_urlsafe_315_into,
};

pub(in crate::bindings::base64) fn translate_bytes(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    let translate = lenient::select_translate_bytes();
    unsafe { translate(input, source0, target0, source1, target1) };
}
