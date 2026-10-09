use super::{
    ConfiguredDecoder, IGNORED_CONFIGURED_VALUE, StrictSpecials, Translation,
    preserves_alphanumeric,
};
use crate::bindings::base64::{
    lenient::lenient_decode_table,
    policy::{Padding, Validation},
    scan::{decode_byte_kernels, scalar::alphanumeric_prefix},
};

mod lenient;
mod scan;
mod snapshots;
mod staging;
mod strict;

fn configured_decoder(
    ignored_bytes: &[u8],
    strict_mode: bool,
    padded: bool,
    canonical: bool,
) -> ConfiguredDecoder {
    let mut table = lenient_decode_table(None);

    for &byte in ignored_bytes {
        if table[usize::from(byte)] >= 64 {
            table[usize::from(byte)] = IGNORED_CONFIGURED_VALUE;
        }
    }

    ConfiguredDecoder {
        table,
        preserves_alphanumeric: preserves_alphanumeric(&table),
        validation: if strict_mode {
            Validation::Strict
        } else {
            Validation::Lenient
        },
        padding: Padding::new(padded),
        canonical,
        alphanumeric_prefix,
        strict_specials: StrictSpecials::new(&table),
        strict_forbidden: StrictSpecials::forbidden(&table),
        translation: Translation::new(&table, None, decode_byte_kernels().translate),
    }
}
