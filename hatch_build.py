"""Hatchling hook that packages the in-tree PyO3 extension."""

from __future__ import annotations

import json
import os
import platform
import subprocess
import sys
from pathlib import Path
from typing import Any

from hatchling.builders.hooks.plugin.interface import BuildHookInterface
from packaging.tags import sys_tags


class CustomBuildHook(BuildHookInterface[Any]):
    """Build the CPython extension wheel."""

    def initialize(self, version: str, build_data: dict[str, Any]) -> None:
        if self.target_name != 'wheel':
            return

        root = Path(self.root)
        wheel_tag = self._wheel_tag()
        env = os.environ.copy()
        configured_target_dir = env.get('CARGO_TARGET_DIR') or env.get('CARGO_LLVM_COV_TARGET_DIR')
        if configured_target_dir:
            target_dir = Path(configured_target_dir)
            if not target_dir.is_absolute():
                target_dir = root / target_dir
        else:
            target_dir = root / 'target' / 'hatch' / wheel_tag
        env['CARGO_TARGET_DIR'] = str(target_dir)
        env['PYO3_PYTHON'] = sys.executable
        result = subprocess.run(
            [
                'cargo',
                'build',
                '--locked',
                '--manifest-path',
                str(root / 'Cargo.toml'),
                '--release',
                '--features',
                'extension-module',
                '--message-format=json-render-diagnostics',
            ],
            check=False,
            cwd=root,
            env=env,
            stdout=subprocess.PIPE,
            text=True,
        )
        result.check_returncode()

        extension = self._cdylib_artifact(result.stdout)
        build_data['force_include'] = {str(extension): f'hashcodecs/_hashcodecs{self._extension_suffix()}'}
        build_data['pure_python'] = False
        build_data['tag'] = wheel_tag

    @staticmethod
    def _cdylib_artifact(messages: str) -> Path:
        suffixes = {'.dll', '.dylib', '.so'}
        for line in messages.splitlines():
            try:
                message = json.loads(line)
            except json.JSONDecodeError:
                continue
            if (
                message.get('reason') == 'compiler-artifact'
                and message.get('target', {}).get('name') == 'hashcodecs'
                and 'cdylib' in message.get('target', {}).get('crate_types', [])
            ):
                for filename in message.get('filenames', []):
                    artifact = Path(filename)
                    if artifact.suffix in suffixes:
                        return artifact
        raise RuntimeError('Cargo did not report the hashcodecs cdylib artifact')

    @staticmethod
    def _extension_suffix() -> str:
        return '.pyd' if platform.system() == 'Windows' else '.so'

    @staticmethod
    def _wheel_tag() -> str:
        return str(next(sys_tags()))
