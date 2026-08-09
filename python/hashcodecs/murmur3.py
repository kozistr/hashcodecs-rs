"""Fast one-shot and incremental MurmurHash3 functions."""

from ._hashcodecs import (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
)

__all__ = [
    "murmur3_32",
    "murmur3_x64_128",
    "murmur3_x64_128_digest",
    "murmur3_x86_32",
    "murmur3_x86_128",
    "murmur3_x86_128_digest",
]
