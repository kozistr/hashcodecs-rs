#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
mod aarch64;
mod scalar;
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;
#[cfg(test)]
pub(super) use scalar::accumulate as accumulate_long_input_scalar;

mod long_engine;

pub(super) use long_engine::*;
