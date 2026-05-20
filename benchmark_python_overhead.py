#!/usr/bin/env python3
"""Benchmark Python overhead around the C FIR downconverter.

This intentionally avoids NumPy and DigitalRF so it can separate the overhead
of Python/ctypes calls and block copying from the C FIR work measured by
benchmark_downconvert.c. It does not measure DigitalRF disk/cache behavior.
"""

import argparse
import ctypes
import math
import time
from pathlib import Path


class ComplexFloat(ctypes.Structure):
    _fields_ = [("re", ctypes.c_float), ("im", ctypes.c_float)]


def load_lib(repo_dir):
    lib_path = Path(repo_dir) / "libdownconvert.so"
    lib = ctypes.CDLL(str(lib_path))
    complex_ptr = ctypes.POINTER(ComplexFloat)
    float_ptr = ctypes.POINTER(ctypes.c_float)
    lib.consume.argtypes = [
        ctypes.c_double,
        ctypes.c_double,
        complex_ptr,
        ctypes.c_int,
        complex_ptr,
        complex_ptr,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_double,
        ctypes.c_double,
        float_ptr,
        ctypes.c_int,
    ]
    lib.consume.restype = None
    return lib


def fill_inputs(z_in, wfun, sintab, input_len, dec2, tabl):
    for i in range(input_len):
        z_in[i].re = math.sin(0.013 * i) + 0.1 * math.cos(0.071 * i)
        z_in[i].im = math.cos(0.017 * i) - 0.1 * math.sin(0.053 * i)

    for i in range(dec2):
        wfun[i] = 0.5 - 0.5 * math.cos(2.0 * math.pi * i / (dec2 - 1))

    for i in range(tabl):
        ph = -2.0 * math.pi * i / tabl
        sintab[i].re = math.cos(ph)
        sintab[i].im = math.sin(ph)


def seconds():
    return time.perf_counter()


def bench_python_loop(total_output_samples, block):
    produced = 0
    idx = 0
    t0 = seconds()
    while produced < total_output_samples:
        n_out = min(block, total_output_samples - produced)
        idx += n_out
        produced += n_out
    return seconds() - t0, idx


def bench_ctypes_noop(total_output_samples, block, z_out, zd):
    produced = 0
    t0 = seconds()
    while produced < total_output_samples:
        n_out = min(block, total_output_samples - produced)
        ctypes.memmove(
            ctypes.byref(zd, produced * ctypes.sizeof(ComplexFloat)),
            z_out,
            n_out * ctypes.sizeof(ComplexFloat),
        )
        produced += n_out
    return seconds() - t0


def bench_input_copy(total_output_samples, block, dec, dec2, source, z_in):
    produced = 0
    t0 = seconds()
    while produced < total_output_samples:
        n_out = min(block, total_output_samples - produced)
        n_in = n_out * dec + dec2
        ctypes.memmove(z_in, source, n_in * ctypes.sizeof(ComplexFloat))
        produced += n_out
    return seconds() - t0


def bench_consume(total_output_samples, block, dec, dec2, dt, f0, rate, lib,
                  sintab, z_in, z_out, wfun, n_threads, copy_output):
    produced = 0
    chirpt = 0.0
    zd = (ComplexFloat * total_output_samples)()
    t0 = seconds()
    while produced < total_output_samples:
        n_out = min(block, total_output_samples - produced)
        lib.consume(
            chirpt,
            dt,
            sintab,
            len(sintab),
            z_in,
            z_out,
            n_out,
            dec,
            dec2,
            f0,
            rate,
            wfun,
            n_threads,
        )
        if copy_output:
            ctypes.memmove(
                ctypes.byref(zd, produced * ctypes.sizeof(ComplexFloat)),
                z_out,
                n_out * ctypes.sizeof(ComplexFloat),
            )
        chirpt += n_out * dec * dt
        produced += n_out
    return seconds() - t0


def main():
    parser = argparse.ArgumentParser(
        description="Benchmark Python overhead around libdownconvert.consume()."
    )
    parser.add_argument("--total-output-samples", type=int, default=120000)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--dec", type=int, default=625)
    parser.add_argument("--filter-len", type=int, default=2)
    parser.add_argument("--sample-rate", type=float, default=25e6)
    parser.add_argument("--center-frequency", type=float, default=12.5e6)
    parser.add_argument("--chirp-rate", type=float, default=500.0084e3)
    parser.add_argument(
        "--blocks",
        default="125,250,500,1000,2000,4000,8000,16000",
        help="Comma separated downconverted block sizes.",
    )
    args = parser.parse_args()

    repo_dir = Path(__file__).resolve().parent
    lib = load_lib(repo_dir)
    dec2 = args.dec * args.filter_len
    dt = 1.0 / args.sample_rate
    f0 = -args.center_frequency
    rate = args.chirp_rate
    tabl = 8192
    blocks = [int(v) for v in args.blocks.split(",") if v]
    max_block = max(blocks)
    max_input = max_block * args.dec + dec2

    z_in = (ComplexFloat * max_input)()
    source = (ComplexFloat * max_input)()
    z_out = (ComplexFloat * max_block)()
    sintab = (ComplexFloat * tabl)()
    wfun = (ctypes.c_float * dec2)()
    fill_inputs(z_in, wfun, sintab, max_input, dec2, tabl)
    fill_inputs(source, wfun, sintab, max_input, dec2, tabl)

    print(
        "Python overhead benchmark: total_output_samples=%d dec=%d "
        "filter_len=%d threads=%d"
        % (args.total_output_samples, args.dec, args.filter_len, args.threads)
    )
    print(
        "block,calls,python_loop_ms,input_copy_ms,output_copy_ms,"
        "consume_ms,consume_plus_output_copy_ms,overhead_without_input_pct,"
        "copy_overhead_pct"
    )

    for block in blocks:
        calls = (args.total_output_samples + block - 1) // block
        t_loop, _ = bench_python_loop(args.total_output_samples, block)
        t_input_copy = bench_input_copy(
            args.total_output_samples, block, args.dec, dec2, source, z_in
        )
        t_output_copy = bench_ctypes_noop(args.total_output_samples, block, z_out,
                                          (ComplexFloat * args.total_output_samples)())
        t_consume = bench_consume(
            args.total_output_samples,
            block,
            args.dec,
            dec2,
            dt,
            f0,
            rate,
            lib,
            sintab,
            z_in,
            z_out,
            wfun,
            args.threads,
            False,
        )
        t_consume_copy = bench_consume(
            args.total_output_samples,
            block,
            args.dec,
            dec2,
            dt,
            f0,
            rate,
            lib,
            sintab,
            z_in,
            z_out,
            wfun,
            args.threads,
            True,
        )
        overhead = max(0.0, t_consume_copy - t_consume)
        overhead_pct = 100.0 * overhead / t_consume_copy if t_consume_copy > 0 else 0.0
        copy_pct = 100.0 * (t_input_copy + t_output_copy) / (
            t_consume_copy + t_input_copy
        )
        print(
            "%d,%d,%.3f,%.3f,%.3f,%.3f,%.3f,%.2f,%.2f"
            % (
                block,
                calls,
                1e3 * t_loop,
                1e3 * t_input_copy,
                1e3 * t_output_copy,
                1e3 * t_consume,
                1e3 * t_consume_copy,
                overhead_pct,
                copy_pct,
            )
        )


if __name__ == "__main__":
    main()
