"""Exercise GC reentrancy in a subprocess because a regression can crash CPython."""

import gc
import sys

import hashcodecs


def check(bits: int, kind: str, length: int) -> None:
    one_shot = getattr(hashcodecs, f'xxh3_{bits}')
    batch = getattr(hashcodecs, f'xxh3_{bits}_batch')
    batch([b'warm up'], 42)
    gc.collect()
    gc.disable()
    items = []
    for index in range(9):
        value = bytes([index + 1]) * length
        item_kind = ('bytes', 'bytearray', 'memoryview')[index % 3] if kind == 'mixed' else kind
        if item_kind == 'bytearray':
            value = bytearray(value)
        elif item_kind == 'memoryview':
            value = memoryview(bytearray(value))
        items.append(value)
    del value
    expected = [one_shot(value, 42) for value in items]
    fired = False
    retained = False

    class Finalizer:
        def __init__(self) -> None:
            self.cycle = self

        def __del__(self) -> None:
            nonlocal fired, retained
            fired = True
            # The list and getrefcount argument account for two references.
            # Exact bytes need an additional native owner during allocation.
            retained = kind != 'bytes' or sys.getrefcount(items[0]) >= 3
            for value in items:
                if isinstance(value, bytearray):
                    value.clear()
                    value.extend(b'changed' * (length + 1))
                elif isinstance(value, memoryview):
                    value[:] = b'x' * len(value)
            items.clear()

    Finalizer()
    # The next tracked allocation is the native result list. Its GC runs the
    # unreachable cycle's finalizer while the batch call is on the C stack.
    gc.set_threshold(1, 0, 0)
    gc.enable()
    actual = batch(items, 42)
    gc.disable()
    assert fired, 'result allocation did not trigger the finalizer'
    assert retained, 'exact bytes owners were not retained during result allocation'
    assert items == []
    assert actual == expected, (bits, kind, length, actual, expected)


if __name__ == '__main__':
    for length in (64, 241, 4097):
        check(int(sys.argv[1]), sys.argv[2], length)
