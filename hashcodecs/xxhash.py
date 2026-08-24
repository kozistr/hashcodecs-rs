"""Fast, canonical XXH3 hashes."""

from ._hashcodecs import (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
)

for _function in (
    xxh3_64,
    xxh3_64_batch,
    xxh3_64_batch_into,
    xxh3_128,
    xxh3_128_batch,
    xxh3_128_batch_into,
):
    _function.__module__ = __name__
del _function

__all__ = [
    'xxh3_64',
    'xxh3_64_batch',
    'xxh3_64_batch_into',
    'xxh3_128',
    'xxh3_128_batch',
    'xxh3_128_batch_into',
]
