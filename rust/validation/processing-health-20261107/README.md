# Ionogram capacity-warning rollout, 2026-11-07

Implementation: chirpsounder2 `9b639ca` (including `a135b80`);
GUI: ionogui2 `695090e` (including `10431c5`).

Validated three operational-counter tests (expiry, startup suppression and
CPU-vs-I/O evidence), nine receiver regressions, nine alert-policy tests and
the existing nine map/navigation deployment checks. Both native TGO and
Ubuntu 24 receiver builds compiled successfully. GUI assets were deployed
from Git and verified against their checksums.

Actual browser inspection covered the live TGO monitoring status and a local
preview of the full propagation map with one injected TEST-prn-Kiruna event.
The event raised a red Needs attention button and marked the unselected KHO
tab. Opening the button showed one missed sounding, six frequency dwells and
CPU/scheduling evidence. This was a local test, never a public/station event.
No live overload was induced to test the warning.

At approximately 10:08 UTC, the public health endpoint carried 15 worker
reports from DOB, seven from TGO and five from KHO, with zero reported losses
at that initial checkpoint. This is a startup snapshot, not a claim about the
preceding uninstrumented period or future reliability. All restarted workers
were active. DOB retained --browser-only --no-upload and its existing delta
publisher; manual deployment transfers were capped at 16 KiB/s. Acquisition,
UHD, spectrum and background-discovery workers were not restarted.

W2NAF SSH port 7374 timed out both directly and via juha.no. KREA reverse port
22220 was unavailable. Their receiver-side monitoring rollout is pending;
the GUI says monitoring has not reported rather than treating this as zero
losses. The same tested release binaries/installer are ready for later access.
