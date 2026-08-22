"""Fast, canonical XXH3 hashes."""

from ._hashcodecs import (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
)

__all__ = [
    'xxh3_64',
    'xxh3_64_batch',
    'xxh3_64_batch_into',
    'xxh3_128',
    'xxh3_128_batch',
    'xxh3_128_batch_into',
]
