"""CPU affinity, topology, and recent-load selection for benchmark workers."""

from __future__ import annotations

import ctypes
import os
import struct
import sys
from dataclasses import dataclass
from pathlib import Path
from time import sleep


@dataclass(frozen=True)
class Cpu:
    index: int
    core: str
    rank: tuple[int, int]
    available: bool


def windows_kernel() -> ctypes.CDLL:
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = ctypes.c_void_p
    kernel.GetProcessAffinityMask.argtypes = (
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_size_t),
        ctypes.POINTER(ctypes.c_size_t),
    )
    kernel.SetProcessAffinityMask.argtypes = (ctypes.c_void_p, ctypes.c_size_t)
    return kernel


def available_cpus() -> set[int]:
    if sys.platform == 'win32':
        kernel = windows_kernel()
        process_mask = ctypes.c_size_t()
        system_mask = ctypes.c_size_t()
        if not kernel.GetProcessAffinityMask(
            kernel.GetCurrentProcess(), ctypes.byref(process_mask), ctypes.byref(system_mask)
        ):
            raise ctypes.WinError(ctypes.get_last_error())
        return {cpu for cpu in range(ctypes.sizeof(process_mask) * 8) if process_mask.value & (1 << cpu)}
    if hasattr(os, 'sched_getaffinity'):
        return set(os.sched_getaffinity(0))
    raise RuntimeError('CPU affinity is unavailable on this platform')


def pin_to_one_cpu(cpu: int | None = None) -> None:
    if sys.platform != 'win32' and not hasattr(os, 'sched_setaffinity'):
        if cpu is not None:
            raise RuntimeError('explicit CPU affinity is unavailable on this platform')
        return
    allowed = available_cpus()
    selected = min(allowed) if cpu is None else cpu
    if selected not in allowed:
        raise ValueError(f'CPU {selected} is outside the process affinity mask')
    if sys.platform == 'win32':
        kernel = windows_kernel()
        if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), 1 << selected):
            raise ctypes.WinError(ctypes.get_last_error())
    else:
        os.sched_setaffinity(0, {selected})


def topology() -> list[Cpu]:
    allowed = available_cpus()
    if sys.platform == 'win32':
        kernel = windows_kernel()
        if kernel.GetActiveProcessorGroupCount() != 1:
            raise RuntimeError('automatic CPU selection requires one Windows processor group; use --cpu')
        query = kernel.GetSystemCpuSetInformation
        query.argtypes = (
            ctypes.c_void_p,
            ctypes.c_ulong,
            ctypes.POINTER(ctypes.c_ulong),
            ctypes.c_void_p,
            ctypes.c_ulong,
        )
        size = ctypes.c_ulong()
        query(None, 0, ctypes.byref(size), kernel.GetCurrentProcess(), 0)
        if not size.value:
            raise ctypes.WinError(ctypes.get_last_error())
        buffer = ctypes.create_string_buffer(size.value)
        if not query(buffer, size.value, ctypes.byref(size), kernel.GetCurrentProcess(), 0):
            raise ctypes.WinError(ctypes.get_last_error())
        cpus = []
        offset = 0
        while offset < size.value:
            length, kind = struct.unpack_from('<II', buffer, offset)
            if length < 8 or offset + length > size.value:
                raise RuntimeError('invalid Windows CPU-set record')
            if kind == 0 and length >= 32:
                group, index, core, _, _, efficiency, flags = struct.unpack_from('<H6B', buffer, offset + 12)
                available = group == 0 and index in allowed and not (flags & 1 or (flags & 2 and not flags & 4))
                cpus.append(Cpu(index, f'{group}:{core}', (efficiency, 0), available))
            offset += length
        return cpus
    if sys.platform == 'linux':
        cpus = []
        for path in Path('/sys/devices/system/cpu').glob('cpu[0-9]*'):
            online = path / 'online'
            if online.exists() and online.read_text().strip() == '0':
                continue
            index = int(path.name[3:])
            siblings = (path / 'topology/thread_siblings_list').read_text().strip()
            rank = []
            for relative in ('topology/core_type', 'cpu_capacity', 'cpufreq/cpuinfo_max_freq'):
                value = path / relative
                rank.append(int(value.read_text()) if value.exists() else 0)
            cpus.append(Cpu(index, siblings, (rank[0], rank[1] or rank[2]), index in allowed))
        return cpus
    raise RuntimeError('automatic CPU selection supports Windows and Linux')


def cpu_times() -> dict[int, tuple[int, int]]:
    if sys.platform == 'win32':
        query = ctypes.WinDLL('ntdll').NtQuerySystemInformation
        query.argtypes = (ctypes.c_ulong, ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_ulong))
        query.restype = ctypes.c_long
        record = struct.Struct('<qqqqqI4x')
        buffer = ctypes.create_string_buffer((os.cpu_count() or 1) * record.size)
        size = ctypes.c_ulong()
        status = query(8, buffer, len(buffer), ctypes.byref(size))
        if status < 0:
            raise OSError(f'cannot read CPU load: NTSTATUS 0x{status & 0xFFFFFFFF:08x}')
        return {
            index: (kernel + user, idle)
            for index, (idle, kernel, user, _, _, _) in enumerate(record.iter_unpack(buffer.raw[: size.value]))
        }
    if sys.platform == 'linux':
        result = {}
        for line in Path('/proc/stat').read_text().splitlines():
            fields = line.split()
            if fields and fields[0].startswith('cpu') and fields[0][3:].isdigit():
                times = [int(value) for value in fields[1:9]]
                result[int(fields[0][3:])] = (sum(times), times[3] + times[4])
        return result
    raise RuntimeError('per-CPU load is unavailable on this platform')


def select_cpus(limit: int) -> list[int]:
    cpus = topology()
    before = cpu_times()
    sleep(0.5)
    after = cpu_times()
    loads = {}
    for cpu in cpus:
        total, idle = before.get(cpu.index, (0, 0))
        new_total, new_idle = after.get(cpu.index, (0, 0))
        elapsed = new_total - total
        loads[cpu.index] = 1 - (new_idle - idle) / elapsed if elapsed > 0 else 1.0
    available = [cpu for cpu in cpus if cpu.available]
    if not available:
        return []
    rank = max(cpu.rank for cpu in available)
    cores: dict[str, list[Cpu]] = {}
    for cpu in cpus:
        cores.setdefault(cpu.core, []).append(cpu)
    candidates = []
    for siblings in cores.values():
        load = max(loads[cpu.index] for cpu in siblings)
        eligible = [cpu.index for cpu in siblings if cpu.available and cpu.rank == rank]
        if eligible and load <= 0.2:
            candidates.append((load, min(eligible)))
    return [index for _, index in sorted(candidates)[:limit]]
