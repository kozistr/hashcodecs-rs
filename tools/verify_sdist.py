"""Build and verify hashcodecs from an extracted source distribution."""

from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath


def run(*command: str, cwd: Path) -> None:
    """Run a verification command with a readable transcript."""
    print(f'+ {shlex.join(command)}', flush=True)
    subprocess.run(command, check=True, cwd=cwd)


def build_sdist(project_root: Path, destination: Path) -> Path:
    """Build exactly one source distribution into ``destination``."""
    destination.mkdir()
    run(
        'uv',
        'build',
        '--sdist',
        '--python',
        sys.executable,
        '--no-sources',
        '--out-dir',
        str(destination),
        cwd=project_root,
    )
    archives = list(destination.glob('*.tar.gz'))
    if len(archives) != 1:
        raise RuntimeError(f'expected one source distribution, found {len(archives)}')
    return archives[0]


def extract_sdist(archive_path: Path, destination: Path) -> Path:
    """Safely extract an sdist containing exactly one top-level directory."""
    destination.mkdir()
    with tarfile.open(archive_path, 'r:gz') as archive:
        members = archive.getmembers()
        roots: set[str] = set()
        for member in members:
            path = PurePosixPath(member.name)
            if path.is_absolute() or '..' in path.parts or not path.parts:
                raise RuntimeError(f'unsafe source distribution path: {member.name!r}')
            if not (member.isfile() or member.isdir()):
                raise RuntimeError(f'unsupported source distribution entry: {member.name!r}')
            roots.add(path.parts[0])
        if len(roots) != 1:
            raise RuntimeError(f'expected one source root, found {len(roots)}')
        archive.extractall(destination, members=members)

    source_root = destination / roots.pop()
    if not source_root.is_dir():
        raise RuntimeError('source distribution root is not a directory')
    return source_root


def venv_python(venv: Path) -> Path:
    """Return the Python executable created by ``uv venv``."""
    if os.name == 'nt':
        return venv / 'Scripts' / 'python.exe'
    return venv / 'bin' / 'python'


def verify_sdist(archive_path: Path, workspace: Path) -> None:
    """Test the Rust crate and installed wheel contained in ``archive_path``."""
    source_root = extract_sdist(archive_path, workspace / 'source')
    wheel_dir = workspace / 'wheel'
    venv = workspace / 'venv'

    run('cargo', 'test', '--lib', '--locked', cwd=source_root)
    run('uv', 'build', '--wheel', '--python', sys.executable, '--out-dir', str(wheel_dir), cwd=source_root)
    run('uv', 'venv', '--python', sys.executable, str(venv), cwd=workspace)

    python = venv_python(venv)
    run(
        'uv',
        'pip',
        'install',
        '--python',
        str(python),
        '--no-index',
        '--find-links',
        str(wheel_dir),
        'hashcodecs',
        cwd=workspace,
    )
    run(
        str(python),
        '-I',
        '-c',
        (
            'import hashcodecs; '
            "assert hashcodecs.b64decode(hashcodecs.b64encode(b'hello')) == b'hello'; "
            "assert hashcodecs.murmur3_32(b'hello') == 0x248bfa47; "
            "assert hashcodecs.xxh3_64(b'') == 0x2d06800538d394c2"
        ),
        cwd=workspace,
    )


def main() -> None:
    """Build an sdist when needed and verify it in a temporary workspace."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('sdist', nargs='?', type=Path, help='existing .tar.gz source distribution')
    arguments = parser.parse_args()

    project_root = Path(__file__).resolve().parents[1]

    with tempfile.TemporaryDirectory(prefix='hashcodecs-sdist-') as temporary:
        workspace = Path(temporary)
        archive_path = (
            arguments.sdist.resolve(strict=True)
            if arguments.sdist is not None
            else build_sdist(project_root, workspace / 'dist')
        )
        verify_sdist(archive_path, workspace)


if __name__ == '__main__':
    main()
