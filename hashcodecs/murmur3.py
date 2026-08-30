# tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.
"""One-shot and incremental MurmurHash3 functions."""

from ._hashcodecs import (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
)

for _public_api in (
    murmur3_32,
    murmur3_x64_128,
    murmur3_x64_128_digest,
    murmur3_x86_32,
    murmur3_x86_128,
    murmur3_x86_128_digest,
):
    _public_api.__module__ = __name__
del _public_api

__all__ = [
    'murmur3_32',
    'murmur3_x64_128',
    'murmur3_x64_128_digest',
    'murmur3_x86_32',
    'murmur3_x86_128',
    'murmur3_x86_128_digest',
]
