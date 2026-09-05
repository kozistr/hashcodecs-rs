"""Build and install the current source wheel into the active test interpreter."""

import subprocess
import sys
import tempfile
from pathlib import Path


def main() -> None:
    root = Path(__file__).resolve().parents[1]
    # A fresh directory prevents older versions or interpreter builds in dist/
    # from being selected instead of the wheel just built from this checkout.
    with tempfile.TemporaryDirectory(prefix='hashcodecs-wheel-') as temporary:
        subprocess.run(
            ['uv', 'build', '--wheel', '--python', sys.executable, '--out-dir', temporary],
            cwd=root,
            check=True,
        )
        wheels = list(Path(temporary).glob('*.whl'))
        if len(wheels) != 1:
            raise RuntimeError(f'expected one current-source wheel, found {len(wheels)}')
        subprocess.run(
            ['uv', 'pip', 'install', '--python', sys.executable, '--reinstall', '--no-deps', str(wheels[0])],
            cwd=root,
            check=True,
        )


if __name__ == '__main__':
    main()
