"""Check retained Base64 batch inputs when output allocation triggers GC."""

import gc

import hashcodecs


def check(decode: bool, length: int) -> None:
    batch = hashcodecs.b64decode_batch if decode else hashcodecs.b64encode_batch
    batch([b'YWJj'])
    gc.collect()
    gc.disable()
    payloads = [bytes([index + 1]) * length for index in range(64)]
    items = [hashcodecs.b64encode(value) for value in payloads] if decode else payloads.copy()
    expected = payloads if decode else [hashcodecs.b64encode(value) for value in items]
    fired = False
    replacements = []

    class Finalizer:
        def __init__(self) -> None:
            self.cycle = self

        def __del__(self) -> None:
            nonlocal fired
            fired = True
            items.clear()
            replacements.extend(bytes([index + 128]) * length for index in range(64))

    if not decode:
        payloads.clear()
    Finalizer()
    gc.set_threshold(1, 0, 0)
    gc.enable()
    actual = batch(items)
    gc.disable()
    assert fired, 'result allocation did not trigger the finalizer'
    assert items == []
    assert actual == expected


if __name__ == '__main__':
    for decode in (False, True):
        for length in (16, 256, 4096):
            check(decode, length)
