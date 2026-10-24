//! Long-input kernels share `LongSchedule`'s bounds: complete 64-byte data
//! stripes and secret windows within the 192-byte `Secret`, including the final
//! overlapping stripe. SIMD calls additionally require their target features,
//! selected by `LongEngine`; raw helper contracts specify any narrower loads.

#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod aarch64;
mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;
#[cfg(test)]
pub(super) use scalar::accumulate as accumulate_long_input_scalar;

mod long_engine;

pub(super) use long_engine::*;
