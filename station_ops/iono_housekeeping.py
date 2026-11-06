import chirp_config as cc
import fnmatch
import os
import time


def cleanup_ringbuffer_files(data_dir, max_age_s):
    now = time.time()
    patterns = ("rf*.h5", "tmp*rf*.h5")
    for root, _, files in os.walk(data_dir):
        for fname in files:
            if not any(fnmatch.fnmatch(fname, pattern) for pattern in patterns):
                continue
            path = os.path.join(root, fname)
            try:
                age_s = now - os.path.getmtime(path)
                if age_s > max_age_s:
                    os.remove(path)
            except FileNotFoundError:
                pass
            except Exception as e:
                print(f"could not remove {path}: {e}")


def cleanup_parameter_files(output_dir, max_age_s, lock_max_age_s):
    now = time.time()
    patterns = ("par-*.h5", "par-*.h5.done", "par-*.h5.lock")
    for root, _, files in os.walk(output_dir):
        for fname in files:
            if not any(fnmatch.fnmatch(fname, pattern) for pattern in patterns):
                continue
            path = os.path.join(root, fname)
            try:
                age_s = now - os.path.getmtime(path)
                artifact_max_age_s = lock_max_age_s if fname.endswith(".lock") else max_age_s
                if age_s > artifact_max_age_s:
                    os.remove(path)
                    print(f"removed old parameter artifact {path}")
            except FileNotFoundError:
                pass
            except Exception as e:
                print(f"could not remove {path}: {e}")


def housekeeping(conf):
    # Archive traversal is costly on NTFS/FUSE. Ring cleanup must remain fast,
    # but expired parameter artifacts do not need a full scan every second.
    next_parameter_cleanup = 0.0
    while True:
        if conf.ringbuffer_cleanup:
            max_age_s = getattr(conf, "ringbuffer_max_age_sec", conf.ringbuffer_max_age_min * 60)
            print("cleaning files older than %d seconds" % (max_age_s))
            cleanup_ringbuffer_files(conf.data_dir, max_age_s)
        if time.monotonic() >= next_parameter_cleanup:
            cleanup_parameter_files(
                conf.output_dir,
                conf.parameter_file_retention_sec,
                conf.parameter_file_lock_timeout_sec)
            next_parameter_cleanup = time.monotonic() + 300.0
        time.sleep(1)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Housekeeping program")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()
    conf=cc.chirp_config(args.config)
    housekeeping(conf)
