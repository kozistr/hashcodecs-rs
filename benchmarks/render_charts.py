"""Render the checked-in benchmark measurements as dependency-free SVG charts."""

from __future__ import annotations

import csv
import html
import math
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / 'docs' / 'benchmarks'

COLORS = {
    'hashcodecs': '#007f73',
    'base64': '#356ac3',
    'base64 turbo': '#a86f00',
    'murmur3': '#356ac3',
    'murmurs': '#8f4a73',
    'fastmurmur3': '#a86f00',
    'mm3h': '#637083',
    'mmh3': '#d55e00',
    'upstream C': '#d55e00',
    'xxhash': '#d55e00',
    'hashcodecs loop': '#356ac3',
    'pybase64': '#8f4a73',
    'CPython': '#637083',
    'returned bytes': '#356ac3',
    'reusable bytearray': '#007f73',
    'returned encode': '#356ac3',
    'reusable encode': '#007f73',
    'returned decode': '#8f4a73',
    'reusable decode': '#d55e00',
    'encode': '#007f73',
    'decode': '#356ac3',
}
FALLBACK_COLORS = ('#007f73', '#d55e00', '#356ac3', '#8f4a73', '#a86f00', '#637083')


@dataclass(frozen=True)
class Panel:
    title: str
    categories: tuple[str, ...]
    series: tuple[tuple[str, tuple[float | None, ...]], ...]


@dataclass(frozen=True)
class Chart:
    filename: str
    title: str
    panels: tuple[Panel, ...]


def panel(title: str, categories: list[str], **series: list[float | None]) -> Panel:
    return Panel(
        title,
        tuple(categories),
        tuple((name.replace('_', ' '), tuple(values)) for name, values in series.items()),
    )


SIZES = ['1 KiB', '4 KiB', '1 MiB', '8 MiB']

CHARTS = (
    Chart(
        'base64-rust.svg',
        'Rust Base64 throughput',
        (
            panel(
                'Standard encode',
                SIZES,
                hashcodecs=[25.19, 35.02, 38.27, 29.24],
                base64=[4.77, 5.40, 4.83, 4.06],
                base64_turbo=[25.11, 33.92, 37.81, 30.99],
            ),
            panel(
                'Standard decode',
                SIZES,
                hashcodecs=[17.92, 25.22, 28.31, 18.79],
                base64=[3.76, 3.98, 3.79, 3.56],
                base64_turbo=[17.37, 23.28, 20.50, 16.87],
            ),
            panel(
                'URL-safe encode',
                SIZES,
                hashcodecs=[25.87, 34.95, 39.68, 29.86],
                base64=[4.79, 5.41, 4.82, 4.08],
                base64_turbo=[25.41, 33.95, 38.29, 31.16],
            ),
            panel(
                'URL-safe decode',
                SIZES,
                hashcodecs=[17.39, 23.84, 27.01, 18.89],
                base64=[3.76, 4.03, 3.81, 3.57],
                base64_turbo=[17.37, 23.16, 20.87, 17.10],
            ),
        ),
    ),
    Chart(
        'murmur3-rust.svg',
        'Rust MurmurHash3 throughput',
        (
            panel(
                'x86 32-bit',
                SIZES,
                hashcodecs=[3.86, 3.77, 3.74, 3.77],
                murmur3=[2.25, 2.26, 2.25, 2.26],
                murmurs=[3.67, 3.51, 3.46, 3.51],
                fastmurmur3=[None] * 4,
                mm3h=[3.93, 3.74, 3.79, 3.74],
            ),
            panel(
                'x86 128-bit',
                SIZES,
                hashcodecs=[7.82, 8.11, 8.38, 8.37],
                murmur3=[4.14, 4.37, 4.35, 4.37],
                murmurs=[7.28, 7.41, 7.57, 7.38],
                fastmurmur3=[None] * 4,
                mm3h=[None] * 4,
            ),
            panel(
                'x64 128-bit',
                SIZES,
                hashcodecs=[9.17, 9.29, 9.31, 9.48],
                murmur3=[5.60, 6.01, 6.04, 5.95],
                murmurs=[8.18, 8.11, 8.22, 8.19],
                fastmurmur3=[8.70, 8.66, 8.68, 8.68],
                mm3h=[7.99, 8.01, 8.19, 7.91],
            ),
        ),
    ),
    Chart(
        'xxh3-rust.svg',
        'Rust XXH3 throughput',
        (
            panel(
                'XXH3-64 one-shot',
                ['64 B', *SIZES],
                hashcodecs=[33.96, 41.65, 62.57, 79.85, 46.26],
                upstream_C=[28.49, 26.29, 39.47, 48.42, 38.70],
            ),
            panel(
                'XXH3-128 one-shot',
                ['64 B', *SIZES],
                hashcodecs=[11.15, 33.88, 57.55, 79.92, 46.21],
                upstream_C=[8.55, 21.69, 37.40, 48.74, 38.60],
            ),
            panel(
                'XXH3-64 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[28.77, 75.64, 89.41, 32.21],
                upstream_C=[20.35, 25.88, 39.18, 16.40],
            ),
            panel(
                'XXH3-128 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[10.62, 64.71, 86.32, 32.09],
                upstream_C=[8.18, 21.49, 36.41, 17.70],
            ),
        ),
    ),
    Chart(
        'base64-python.svg',
        'Python Base64 throughput',
        (
            panel(
                'Standard encode',
                SIZES,
                hashcodecs=[10.78, 23.43, 3.89, 4.62],
                CPython=[0.44, 0.46, 0.40, 0.42],
                pybase64=[5.02, 13.11, 2.68, 2.91],
            ),
            panel(
                'Standard decode',
                SIZES,
                hashcodecs=[8.59, 17.93, 4.47, 4.32],
                CPython=[0.93, 1.08, 0.84, 0.90],
                pybase64=[3.09, 8.01, 3.26, 3.57],
            ),
            panel(
                'URL-safe encode',
                SIZES,
                hashcodecs=[9.98, 22.26, 3.76, 4.46],
                CPython=[0.37, 0.39, 0.33, 0.34],
                pybase64=[0.96, 1.12, 0.84, 0.84],
            ),
            panel(
                'URL-safe decode',
                SIZES,
                hashcodecs=[7.02, 13.75, 4.86, 4.24],
                CPython=[0.47, 0.69, 0.60, 0.59],
                pybase64=[1.13, 1.47, 1.34, 1.38],
            ),
        ),
    ),
    Chart(
        'murmur3-python.svg',
        'Python MurmurHash3 throughput',
        tuple(
            panel(title, SIZES, hashcodecs=ours, mmh3=upstream)
            for title, ours, upstream in (
                ('x86 32-bit one-shot', [3.56, 3.85, 3.99, 3.99], [3.44, 3.72, 3.84, 3.83]),
                ('x86 32-bit incremental', [2.65, 3.43, 3.99, 3.99], [2.86, 3.48, 3.84, 3.84]),
                ('x86 128-bit one-shot', [6.59, 8.48, 9.42, 9.42], [6.69, 8.22, 8.89, 8.86]),
                ('x86 128-bit incremental', [4.01, 7.09, 9.27, 9.27], [0.69, 0.78, 0.80, 0.81]),
                ('x64 128-bit one-shot', [6.95, 9.13, 10.08, 10.03], [7.79, 9.48, 10.26, 10.26]),
                ('x64 128-bit incremental', [4.34, 7.79, 10.11, 10.10], [5.32, 8.05, 9.36, 8.20]),
            )
        ),
    ),
    Chart(
        'xxh3-python.svg',
        'Python XXH3 throughput',
        (
            panel(
                'XXH3-64 one-shot', SIZES, hashcodecs=[16.31, 41.23, 78.41, 46.46], xxhash=[13.42, 29.23, 47.10, 38.50]
            ),
            panel(
                'XXH3-128 one-shot', SIZES, hashcodecs=[10.33, 30.28, 78.46, 46.42], xxhash=[9.27, 22.42, 46.74, 38.79]
            ),
            panel(
                'XXH3-64 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[2.87, 29.19, 58.57, 37.00],
                xxhash=[2.15, 14.55, 30.50, 18.39],
            ),
            panel(
                'XXH3-128 batch (32 items)',
                ['64 B', '1 KiB', '4 KiB', '1 MiB'],
                hashcodecs=[1.17, 15.48, 40.49, 37.16],
                xxhash=[1.01, 9.87, 23.87, 13.75],
            ),
        ),
    ),
    Chart(
        'base64-python-reusable.svg',
        'Reusable Python Base64 buffers',
        (
            panel('Standard encode', SIZES, hashcodecs=[13.70, 26.46, 39.04, 29.24]),
            panel('Standard decode', SIZES, hashcodecs=[9.76, 19.53, 29.07, 17.45]),
            panel('URL-safe encode', SIZES, hashcodecs=[12.82, 25.39, 37.25, 29.32]),
            panel('URL-safe decode', SIZES, hashcodecs=[8.22, 14.83, 19.89, 16.55]),
        ),
    ),
    Chart(
        'base64-python-memoryview.svg',
        'Python Base64 memoryview inputs',
        (
            panel(
                'Encode',
                SIZES,
                returned_bytes=[5.83, 18.73, 3.02, 4.48],
                reusable_bytearray=[6.66, 21.77, 39.73, 30.25],
            ),
            panel(
                'Decode',
                SIZES,
                returned_bytes=[5.26, 15.04, 3.70, 4.06],
                reusable_bytearray=[5.70, 16.52, 29.01, 19.08],
            ),
        ),
    ),
    Chart(
        'base64-python-batch.svg',
        'Python Base64 batch throughput',
        tuple(
            panel(
                title, ['8', '64', '1,024'], hashcodecs=ours, hashcodecs_loop=loop, pybase64=pybase64, CPython=cpython
            )
            for title, ours, loop, pybase64, cpython in (
                ('16 B encode', [0.45, 0.60, 0.61], [0.30, 0.33, 0.33], [0.12, 0.13, 0.13], [0.18, 0.19, 0.19]),
                ('16 B decode', [0.41, 0.55, 0.55], [0.22, 0.24, 0.24], [0.08, 0.08, 0.08], [0.12, 0.12, 0.13]),
                ('256 B encode', [6.30, 7.34, 7.28], [4.33, 4.41, 4.34], [1.86, 1.82, 1.87], [0.38, 0.39, 0.38]),
                ('256 B decode', [5.84, 7.12, 6.67], [3.26, 3.39, 3.27], [1.17, 1.22, 1.23], [0.74, 0.75, 0.72]),
                ('4 KiB encode', [24.62, 18.26, 6.06], [20.55, 15.76, 2.46], [12.23, 8.99, 2.27], [0.46, 0.45, 0.39]),
                ('4 KiB decode', [19.78, 19.24, 7.71], [16.03, 15.50, 11.21], [7.74, 7.62, 6.66], [1.07, 1.06, 1.05]),
            )
        ),
    ),
    Chart(
        'base64-python-batch-reusable.svg',
        'Reusable Python Base64 batch buffers',
        tuple(
            panel(title, ['8', '64', '1,024'], encode=encode, decode=decode)
            for title, encode, decode in (
                ('16 B items', [0.37, 0.52, 0.54], [0.33, 0.47, 0.50]),
                ('256 B items', [4.77, 6.04, 6.07], [4.41, 5.91, 5.90]),
                ('4 KiB items', [26.13, 26.81, 17.91], [20.30, 22.06, 16.88]),
            )
        ),
    ),
    Chart(
        'base64-python-batch-large.svg',
        'Large Python Base64 batches',
        (
            panel(
                '1 MiB items',
                ['1', '2', '4', '8', '16', '32'],
                returned_encode=[3.14, 3.23, 3.11, 3.04, 2.87, 2.85],
                reusable_encode=[39.83, 20.96, 20.19, 19.46, 13.31, 11.03],
                returned_decode=[3.74, 3.78, 3.80, 3.80, 3.42, 3.28],
                reusable_decode=[29.05, 21.64, 21.06, 19.03, 12.18, 10.49],
            ),
        ),
    ),
    Chart(
        'base64-python-mutable.svg',
        'Mutable Python Base64 inputs',
        (
            panel(
                'Encode',
                SIZES,
                returned_bytes=[10.17, 22.44, 3.01, 5.13],
                reusable_bytearray=[13.77, 26.33, 39.34, 29.80],
            ),
            panel(
                'Decode',
                SIZES,
                returned_bytes=[8.52, 17.68, 3.77, 3.93],
                reusable_bytearray=[9.96, 18.83, 28.95, 18.46],
            ),
        ),
    ),
    Chart(
        'murmur3-python-mutable.svg',
        'Mutable Python MurmurHash3 inputs',
        tuple(
            panel(title, SIZES, hashcodecs=values)
            for title, values in (
                ('x86 32-bit one-shot', [3.34, 3.71, 3.93, 3.88]),
                ('x86 32-bit incremental', [2.71, 3.53, 3.85, 3.87]),
                ('x86 128-bit one-shot', [5.72, 8.01, 8.92, 9.04]),
                ('x86 128-bit incremental', [4.02, 6.91, 8.76, 8.92]),
                ('x64 128-bit one-shot', [5.82, 8.39, 9.83, 9.84]),
                ('x64 128-bit incremental', [4.47, 7.58, 9.82, 9.89]),
            )
        ),
    ),
)


def esc(value: object) -> str:
    return html.escape(str(value), quote=True)


def nice_max(value: float) -> float:
    rough = value / 4
    exponent = 10 ** math.floor(math.log10(rough)) if rough else 1
    step = next(candidate * exponent for candidate in (1, 2, 5, 10) if candidate * exponent >= rough)
    return math.ceil(value / step) * step


def color(name: str, index: int) -> str:
    return COLORS.get(name, FALLBACK_COLORS[index % len(FALLBACK_COLORS)])


OFFICIAL_SERIES = {
    'base64-rust.svg': 'base64',
    'murmur3-rust.svg': 'murmur3',
    'xxh3-rust.svg': 'upstream C',
    'base64-python.svg': 'CPython',
    'murmur3-python.svg': 'mmh3',
    'xxh3-python.svg': 'xxhash',
    'base64-python-batch.svg': 'CPython',
}


def render(chart: Chart) -> str:
    width = 1280
    columns = 1 if len(chart.panels) == 1 else 2
    panel_width = 1180 if columns == 1 else 570
    panel_height = 330
    rows = math.ceil(len(chart.panels) / columns)
    height = 100 + rows * panel_height + 30
    chunks = [
        (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}" role="img" aria-labelledby="title desc">'
        ),
        f'<title id="title">{esc(chart.title)}</title>',
        f'<desc id="desc">{esc(chart.title)} in GiB/s. Higher is better.</desc>',
        '<rect width="100%" height="100%" fill="#ffffff"/>',
        (
            '<text x="50" y="48" fill="#172033" font-family="Segoe UI,Arial,sans-serif" '
            f'font-size="28" font-weight="700">{esc(chart.title)}</text>'
        ),
        (
            '<text x="50" y="74" fill="#637083" font-family="Segoe UI,Arial,sans-serif" '
            'font-size="14">Throughput (GiB/s), higher is better</text>'
        ),
    ]

    for panel_index, spec in enumerate(chart.panels):
        column = panel_index % columns
        row = panel_index // columns
        origin_x = 50 + column * 610
        origin_y = 100 + row * panel_height
        plot_x = origin_x + 58
        plot_y = origin_y + 70
        plot_width = panel_width - 78
        plot_height = 210
        available = [value for _, values in spec.series for value in values if value is not None]
        axis_max = nice_max(max(available))

        chunks.extend(
            (
                (
                    f'<text x="{origin_x}" y="{origin_y + 24}" fill="#172033" '
                    'font-family="Segoe UI,Arial,sans-serif" '
                    f'font-size="18" font-weight="600">{esc(spec.title)}</text>'
                ),
                (
                    f'<line x1="{plot_x}" y1="{plot_y + plot_height}" '
                    f'x2="{plot_x + plot_width}" y2="{plot_y + plot_height}" stroke="#9aa5b4"/>'
                ),
            )
        )

        legend_x = origin_x
        for series_index, (name, _) in enumerate(spec.series):
            series_color = color(name, series_index)
            chunks.extend(
                (
                    (
                        f'<line x1="{legend_x}" y1="{origin_y + 48}" x2="{legend_x + 20}" '
                        f'y2="{origin_y + 48}" stroke="{series_color}" stroke-width="3"/>'
                    ),
                    (
                        f'<text x="{legend_x + 26}" y="{origin_y + 53}" fill="#465263" '
                        'font-family="Segoe UI,Arial,sans-serif" '
                        f'font-size="12">{esc(name)}</text>'
                    ),
                )
            )
            legend_x += 34 + len(name) * 7

        for tick in range(5):
            value = axis_max * tick / 4
            y = plot_y + plot_height - plot_height * tick / 4
            chunks.extend(
                (
                    (f'<line x1="{plot_x}" y1="{y:.1f}" x2="{plot_x + plot_width}" y2="{y:.1f}" stroke="#e1e6ed"/>'),
                    (
                        f'<text x="{plot_x - 10}" y="{y + 4:.1f}" text-anchor="end" fill="#637083" '
                        f'font-family="Segoe UI,Arial,sans-serif" font-size="11">{value:g}</text>'
                    ),
                )
            )

        x_step = plot_width / max(len(spec.categories) - 1, 1)
        for category_index, category in enumerate(spec.categories):
            x = plot_x + category_index * x_step
            chunks.append(
                f'<text x="{x:.1f}" y="{plot_y + plot_height + 24}" text-anchor="middle" '
                'fill="#465263" font-family="Segoe UI,Arial,sans-serif" '
                f'font-size="12">{esc(category)}</text>'
            )

        for series_index, (name, values) in enumerate(spec.series):
            series_color = color(name, series_index)
            official = OFFICIAL_SERIES.get(chart.filename)
            official_values = next(
                (candidate for candidate_name, candidate in spec.series if candidate_name == official), None
            )
            points = []
            for category_index, value in enumerate(values):
                if value is None:
                    continue
                x = plot_x + category_index * x_step
                y = plot_y + plot_height - value / axis_max * plot_height
                points.append((x, y, value, spec.categories[category_index]))
            if len(points) > 1:
                path = ' '.join(f'{x:.1f},{y:.1f}' for x, y, _, _ in points)
                chunks.append(
                    f'<polyline points="{path}" fill="none" stroke="{series_color}" '
                    'stroke-width="3" stroke-linejoin="round" stroke-linecap="round"/>'
                )
            for x, y, value, category in points:
                chunks.append(
                    f'<circle cx="{x:.1f}" cy="{y:.1f}" r="4.5" fill="{series_color}" '
                    'stroke="#ffffff" stroke-width="2">'
                    f'<title>{esc(spec.title)}: {esc(name)}, {esc(category)}, '
                    f'{value:.2f} GiB/s</title></circle>'
                )
                # Label every point, alternating above and below each series.
                offset = (-10 - 12 * (series_index // 2)) if series_index % 2 == 0 else (17 + 12 * (series_index // 2))
                label = f'{value:.2f}'
                if name == 'hashcodecs' and official_values is not None:
                    official_value = official_values[spec.categories.index(category)]
                    if official_value is not None and official_value > 0:
                        label += f' ({value / official_value:.2f}x)'
                chunks.append(
                    f'<text x="{x:.1f}" y="{y + offset:.1f}" text-anchor="middle" '
                    f'fill="{series_color}" font-family="Segoe UI,Arial,sans-serif" '
                    f'font-size="11" font-weight="600">{label}</text>'
                )

    chunks.append('</svg>')
    return '\n'.join(chunks) + '\n'


def write_csv() -> None:
    with (OUTPUT / 'results.csv').open('w', newline='', encoding='utf-8') as output:
        writer = csv.writer(output, lineterminator='\n')
        writer.writerow(('chart', 'panel', 'input', 'implementation', 'gib_per_second'))
        for chart in CHARTS:
            for spec in chart.panels:
                for name, values in spec.series:
                    for category, value in zip(spec.categories, values, strict=True):
                        if value is not None:
                            writer.writerow((chart.title, spec.title, category, name, f'{value:.2f}'))


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for chart in CHARTS:
        (OUTPUT / chart.filename).write_text(render(chart), encoding='utf-8', newline='\n')
    write_csv()


if __name__ == '__main__':
    main()
