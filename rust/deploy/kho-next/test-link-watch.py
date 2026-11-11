"""Exercise cable transitions and callback failures without network changes."""
from pathlib import Path
import subprocess
import tempfile
import time

script = Path(__file__).with_name("link-watch.sh")
with tempfile.TemporaryDirectory() as directory:
    root = Path(directory)
    carrier, log, callback = (root / name for name in ("carrier", "log", "callback"))
    carrier.write_text("0\n")
    callback.write_text(f'#!/bin/bash\necho "$1" >> "{log}"\n'
                        f'if [[ -f "{root / "fail"}" ]]; then rm "{root / "fail"}"; exit 1; fi\n')
    callback.chmod(0o755)
    process = subprocess.Popen(["bash", str(script), str(carrier), str(callback), "0.05"], stdout=subprocess.DEVNULL)
    def expect(events):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            if log.exists() and log.read_text().splitlines() == events:
                return
            time.sleep(0.02)
        raise AssertionError(log.read_text() if log.exists() else "no callback")
    try:
        expect(["down"])
        carrier.write_text("1\n")
        expect(["down", "up"])
        time.sleep(0.25)
        assert log.read_text().splitlines() == ["down", "up"]
        carrier.unlink()  # Missing interface must stop, never keep publishing.
        expect(["down", "up", "down"])
        (root / "fail").touch()
        carrier.write_text("1\n")
        expect(["down", "up", "down", "up", "up"])
    finally:
        process.terminate()
        process.wait(timeout=5)
print("Cable transitions, idle de-duplication, missing interface and callback retry passed")
