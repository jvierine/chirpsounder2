#!/usr/bin/env python3
"""Plot chirp downconversion anti-alias filter responses.

This reproduces the three filter choices used by chirp_lib.chirp_downconvert:

* fir: Hann-windowed sinc, length filter_len * decimation
* boxcar: one decimation-length moving average
* cic: cascaded moving-average response, with cic_stages stages

The script writes SVG files and avoids NumPy/Matplotlib so it can run on
minimal receiver machines.
"""

import argparse
import cmath
import math
import os
from typing import Dict, Iterable, List, Sequence, Tuple


Color = Tuple[int, int, int]
Series = Tuple[str, Color, List[float]]


COLORS: Dict[str, Color] = {
    "FIR": (33, 101, 172),
    "Boxcar": (178, 24, 43),
    "CIC": (35, 139, 69),
}


def hann(n: int, length: int) -> float:
    if length <= 1:
        return 1.0
    return 0.5 - 0.5 * math.cos(2.0 * math.pi * n / float(length - 1))


def fir_taps(decimation: int, filter_len: int) -> List[float]:
    """Return the Hann-windowed sinc taps used by chirp_lib.py."""
    length = decimation * filter_len
    omega0 = 2.0 * math.pi / float(decimation)
    taps = []
    for n in range(length):
        m = float(n - decimation)
        # Match chirp_lib.py, which offsets by a tiny epsilon to avoid 0/0.
        x = m + 1e-6
        taps.append(hann(n, length) * math.sin(omega0 * x) / (math.pi * x))
    return taps


def fir_response_db(taps: Sequence[float], freq: float) -> float:
    dc_gain = sum(taps)
    response = 0j
    for n, tap in enumerate(taps):
        response += tap * cmath.exp(-2j * math.pi * freq * n)
    return db(abs(response) / abs(dc_gain))


def boxcar_amplitude(decimation: int, freq: float) -> float:
    denominator = math.sin(math.pi * freq)
    if abs(denominator) < 1e-15:
        return 1.0
    numerator = math.sin(math.pi * freq * decimation)
    return abs(numerator / (float(decimation) * denominator))


def db(amplitude: float, floor: float = -120.0) -> float:
    if amplitude <= 0.0:
        return floor
    return max(floor, 20.0 * math.log10(amplitude))


def responses(decimation: int,
              filter_len: int,
              cic_stages: int,
              sample_rate: float,
              x_values: Iterable[float]) -> List[Series]:
    taps = fir_taps(decimation, filter_len)
    fir = []
    boxcar = []
    cic = []
    for freq_khz in x_values:
        freq = freq_khz * 1e3 / sample_rate
        b = boxcar_amplitude(decimation, freq)
        fir.append(fir_response_db(taps, freq))
        boxcar.append(db(b))
        cic.append(db(b ** cic_stages))
    return [
        ("FIR", COLORS["FIR"], fir),
        ("Boxcar", COLORS["Boxcar"], boxcar),
        ("CIC", COLORS["CIC"], cic),
    ]


def polyline(points: Sequence[Tuple[float, float]],
             color: Color,
             width: float = 2.0) -> str:
    rgb = "rgb(%d,%d,%d)" % color
    coords = " ".join("%0.2f,%0.2f" % (x, y) for x, y in points)
    return (
        '<polyline points="%s" fill="none" stroke="%s" '
        'stroke-width="%0.1f" stroke-linejoin="round" stroke-linecap="round" />'
        % (coords, rgb, width)
    )


def write_svg(path: str,
              title: str,
              x_values: Sequence[float],
              series: Sequence[Series],
              y_min: float,
              y_max: float,
              x_label: str,
              y_label: str) -> None:
    width = 980
    height = 560
    left = 78
    right = 26
    top = 54
    bottom = 68
    plot_w = width - left - right
    plot_h = height - top - bottom
    x_min = min(x_values)
    x_max = max(x_values)

    def sx(x: float) -> float:
        return left + (x - x_min) / (x_max - x_min) * plot_w

    def sy(y: float) -> float:
        y = min(y_max, max(y_min, y))
        return top + (y_max - y) / (y_max - y_min) * plot_h

    svg = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" viewBox="0 0 %d %d">'
        % (width, height, width, height),
        '<rect width="100%" height="100%" fill="#ffffff" />',
        '<text x="%d" y="30" font-family="Arial, sans-serif" font-size="22" font-weight="700">%s</text>'
        % (left, title),
        '<rect x="%d" y="%d" width="%d" height="%d" fill="#fafafa" stroke="#222" stroke-width="1" />'
        % (left, top, plot_w, plot_h),
    ]

    x_ticks = [x_min + i * (x_max - x_min) / 8.0 for i in range(9)]
    y_step = 10.0 if (y_max - y_min) <= 40.0 else 20.0
    y_ticks = []
    tick = math.ceil(y_min / y_step) * y_step
    while tick <= y_max + 1e-9:
        y_ticks.append(tick)
        tick += y_step

    for x in x_ticks:
        px = sx(x)
        svg.append('<line x1="%0.2f" y1="%d" x2="%0.2f" y2="%d" stroke="#ddd" />'
                   % (px, top, px, top + plot_h))
        svg.append('<text x="%0.2f" y="%d" font-family="Arial, sans-serif" font-size="12" text-anchor="middle">%0.2g</text>'
                   % (px, height - 42, x))

    for y in y_ticks:
        py = sy(y)
        svg.append('<line x1="%d" y1="%0.2f" x2="%d" y2="%0.2f" stroke="#ddd" />'
                   % (left, py, left + plot_w, py))
        svg.append('<text x="%d" y="%0.2f" font-family="Arial, sans-serif" font-size="12" text-anchor="end" dominant-baseline="middle">%0.0f</text>'
                   % (left - 10, py, y))

    for name, color, values in series:
        points = [(sx(x), sy(y)) for x, y in zip(x_values, values)]
        svg.append(polyline(points, color))

    legend_x = left + plot_w - 170
    legend_y = top + 22
    for idx, (name, color, _) in enumerate(series):
        y = legend_y + idx * 24
        svg.append('<line x1="%d" y1="%d" x2="%d" y2="%d" stroke="rgb(%d,%d,%d)" stroke-width="3" />'
                   % (legend_x, y, legend_x + 34, y, color[0], color[1], color[2]))
        svg.append('<text x="%d" y="%d" font-family="Arial, sans-serif" font-size="14" dominant-baseline="middle">%s</text>'
                   % (legend_x + 44, y, name))

    svg.append('<text x="%d" y="%d" font-family="Arial, sans-serif" font-size="15" text-anchor="middle">%s</text>'
               % (left + plot_w / 2, height - 12, x_label))
    svg.append('<text transform="translate(22 %d) rotate(-90)" font-family="Arial, sans-serif" font-size="15" text-anchor="middle">%s</text>'
               % (top + plot_h / 2, y_label))
    svg.append('</svg>')

    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(svg) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Plot chirp downconversion filter frequency responses.")
    parser.add_argument("--decimation", type=int, default=625)
    parser.add_argument("--filter-len", type=int, default=2)
    parser.add_argument("--cic-stages", type=int, default=2)
    parser.add_argument("--sample-rate", type=float, default=25e6)
    parser.add_argument("--max-frequency-khz", type=float, default=1000.0)
    parser.add_argument("--passband-frequency-khz", type=float, default=None)
    parser.add_argument("--output-dir", default="memos/figures")
    args = parser.parse_args()

    os.makedirs(args.output_dir, exist_ok=True)

    output_nyquist_khz = args.sample_rate / (2.0 * args.decimation) / 1e3
    passband_max_khz = args.passband_frequency_khz
    if passband_max_khz is None:
        passband_max_khz = output_nyquist_khz

    overview_x = [args.max_frequency_khz * i / 1200.0 for i in range(1201)]
    passband_x = [passband_max_khz * i / 600.0 for i in range(601)]
    overview = responses(args.decimation,
                         args.filter_len,
                         args.cic_stages,
                         args.sample_rate,
                         overview_x)
    passband = responses(args.decimation,
                         args.filter_len,
                         args.cic_stages,
                         args.sample_rate,
                         passband_x)

    suffix = "dec%d_cic%d" % (args.decimation, args.cic_stages)
    overview_path = os.path.join(
        args.output_dir, "downconversion_filter_response_%s.svg" % suffix)
    passband_path = os.path.join(
        args.output_dir, "downconversion_filter_passband_%s.svg" % suffix)

    title = "Chirp Downconversion Filter Responses, decimation=%d" % args.decimation
    write_svg(overview_path,
              title,
              overview_x,
              overview,
              -100.0,
              2.0,
              "Frequency (kHz)",
              "Amplitude response (dB)")
    write_svg(passband_path,
              "Passband Droop, decimation=%d" % args.decimation,
              passband_x,
              passband,
              -10.0,
              1.0,
              "Frequency (kHz)",
              "Amplitude response (dB)")

    print("wrote %s" % overview_path)
    print("wrote %s" % passband_path)


if __name__ == "__main__":
    main()
