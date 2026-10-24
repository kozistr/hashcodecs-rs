use core::ffi::c_void;

mod backends;
mod batch;
mod prepared;
mod reference;

fn c_xxh3_64(input: &[u8], seed: u64) -> u64 {
    unsafe {
        xxhash_c_sys::XXH3_64bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
    }
}

fn c_xxh3_128(input: &[u8], seed: u64) -> [u64; 2] {
    let hash = unsafe {
        xxhash_c_sys::XXH3_128bits_withSeed(input.as_ptr().cast::<c_void>(), input.len(), seed)
    };
    [hash.low64, hash.high64]
}
