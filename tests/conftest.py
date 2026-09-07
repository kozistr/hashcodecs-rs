import sys
import threading
import time
from collections.abc import Callable

import pytest

GILProgressAssertion = Callable[[Callable[[], object], object, int], None]


@pytest.fixture
def assert_releases_gil() -> GILProgressAssertion:
    def assert_progress(operation: Callable[[], object], expected: object, repetitions: int) -> None:
        ready = threading.Event()
        start = threading.Event()
        progressed = threading.Event()

        def report_progress() -> None:
            ready.set()
            if start.wait(timeout=5):
                progressed.set()

        worker = threading.Thread(target=report_progress)
        worker.start()
        assert ready.wait(timeout=5)
        previous_switch_interval = sys.getswitchinterval()
        try:
            # Keep the main thread from yielding in the Python code between
            # native calls. The worker can progress only while an extension
            # call explicitly detaches from the interpreter.
            sys.setswitchinterval(10.0)
            start.set()
            result = expected
            deadline = time.monotonic() + 1.0
            for _ in range(repetitions):
                result = operation()
            # Fast native calls can finish before the OS schedules the worker.
            # Keep calling the operation within a bounded window; waiting here
            # would release the GIL and make the assertion a false positive.
            while not progressed.is_set() and time.monotonic() < deadline:
                result = operation()
            progressed_during_call = progressed.is_set()
        finally:
            sys.setswitchinterval(previous_switch_interval)
            worker.join(timeout=5)

        assert not worker.is_alive()
        assert progressed_during_call
        assert result == expected

    return assert_progress
