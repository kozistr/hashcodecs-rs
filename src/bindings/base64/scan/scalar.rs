#[inline(always)]
pub(in crate::bindings::base64) fn is_lenient_symbol(byte: u8, altchars: Option<[u8; 2]>) -> bool {
    byte.wrapping_sub(b'A') <= b'Z' - b'A'
        || byte.wrapping_sub(b'a') <= b'z' - b'a'
        || byte.wrapping_sub(b'0') <= b'9' - b'0'
        || matches!(byte, b'+' | b'/')
        || altchars.is_some_and(|[plus, slash]| byte == plus || byte == slash)
}

pub(super) fn symbol_count_scalar(input: &[u8], altchars: Option<[u8; 2]>) -> usize {
    input
        .iter()
        .filter(|&&byte| is_lenient_symbol(byte, altchars))
        .count()
}

pub(in crate::bindings::base64) unsafe fn alphanumeric_prefix_scalar(input: &[u8]) -> usize {
    input
        .iter()
        .position(|byte| !byte.is_ascii_alphanumeric())
        .unwrap_or(input.len())
}

#[cfg(any(not(target_arch = "aarch64"), test))]
pub(in crate::bindings::base64) unsafe fn symbol_prefix_scalar(
    input: &[u8],
    altchars: Option<[u8; 2]>,
) -> usize {
    input
        .iter()
        .position(|&byte| !is_lenient_symbol(byte, altchars))
        .unwrap_or(input.len())
}

#[cfg(test)]
pub(in crate::bindings::base64) unsafe fn translate_bytes_scalar(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    unsafe { translate_scalar(input, source0, target0, source1, target1) };
}

pub(in crate::bindings::base64) unsafe fn translate_scalar(
    input: &mut [u8],
    source0: u8,
    target0: u8,
    source1: u8,
    target1: u8,
) {
    for byte in input {
        if *byte == source0 {
            *byte = target0;
        } else if *byte == source1 {
            *byte = target1;
        }
    }
}
