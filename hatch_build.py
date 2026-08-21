"""Hatchling hook that packages the in-tree PyO3 extension."""

from __future__ import annotations

import platform
import subprocess
from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface
from packaging.tags import sys_tags


class CustomBuildHook(BuildHookInterface[Any]):
    """Build a version-specific CPython extension wheel."""

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:
        if self.target_name != 'wheel':
            return

        root = Path(self.root)
        subprocess.run(
            [
                'cargo',
                'build',
                '--manifest-path',
                str(root / 'Cargo.toml'),
                '--release',
                '--features',
                'extension-module',
            ],
            check=True,
            cwd=root,
        )

        extension = root / 'target' / 'release' / self._library_name()
        build_data['force_include'] = {str(extension): f'hashcodecs/_hashcodecs{self._extension_suffix()}'}
        build_data['pure_python'] = False
        build_data['tag'] = self._wheel_tag()

    @staticmethod
    def _library_name() -> str:
        system = platform.system()
        if system == 'Windows':
            return 'hashcodecs.dll'
        if system == 'Darwin':
            return 'libhashcodecs.dylib'
        return 'libhashcodecs.so'

    @staticmethod
    def _extension_suffix() -> str:
        return '.pyd' if platform.system() == 'Windows' else '.so'

    @staticmethod
    def _wheel_tag() -> str:
        return str(next(sys_tags()))
