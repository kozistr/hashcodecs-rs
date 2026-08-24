"""Fast one-shot and incremental MurmurHash3 functions."""

from ._hashcodecs import (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
)

for _function in (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
):
    _function.__module__ = __name__
del _function

__all__ = [
    'murmur3_32',
    'murmur3_x64_128',
    'murmur3_x64_128_digest',
    'murmur3_x86_32',
    'murmur3_x86_128',
    'murmur3_x86_128_digest',
]
