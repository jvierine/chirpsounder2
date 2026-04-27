#!/usr/bin/env python3
import argparse
import glob
import os
import sys

import ionowebsync


def expand_inputs(patterns):
    files = []
    for pattern in patterns:
        matches = sorted(glob.glob(pattern))
        if matches:
            files.extend(matches)
        else:
            files.append(pattern)
    return files


def main():
    parser = argparse.ArgumentParser(description="Manually post HDF5 files with ionowebsync.")
    parser.add_argument("paths", nargs="+", help="Files or glob patterns to upload")
    args = parser.parse_args()

    files = expand_inputs(args.paths)
    print("found %d file(s) to upload" % len(files))
    for fname in files:
        print("  %s" % fname)

    ok = 0
    failed = 0

    for fname in files:
        if not os.path.isfile(fname):
            print("FAIL missing file: %s" % fname)
            failed += 1
            continue

        print("posting %s" % fname)
        response = ionowebsync.post_to_server(fname)
        if response is None:
            print("FAIL %s" % fname)
            failed += 1
            continue

        status_line = "OK" if response.ok else "FAIL"
        print("%s %s (HTTP %d)" % (status_line, fname, response.status_code))
        body = response.text.strip()
        if body:
            print(body)

        if response.ok:
            ok += 1
        else:
            failed += 1

    print("summary: %d succeeded, %d failed" % (ok, failed))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
