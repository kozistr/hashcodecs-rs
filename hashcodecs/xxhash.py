# tools/generate_api_metadata.py generates this file from hashcodecs/_hashcodecs.pyi.
"""Canonical XXH3 functions."""

from ._hashcodecs import (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
)

for _public_api in (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
):
    _public_api.__module__ = __name__
del _public_api

__all__ = [
    'xxh3_64',
    'xxh3_64_batch',
    'xxh3_64_batch_into',
    'xxh3_128',
    'xxh3_128_batch',
    'xxh3_128_batch_into',
]
