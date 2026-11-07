# Ionogram deadline-loss monitoring

Live Digisonde, scheduled LFM, long-code PRN and VERSO workers publish small
atomic operational reports under the receiver user's local state directory,
`iono-processing/`. Receiver-local iono-plots carries a bounded summary in
existing HDF5 browser-bundle health metadata. No extra uploader, archive scan,
raw-voltage transfer or Python process is involved.

The rolling 15-minute report separates entirely skipped scheduled soundings
from skipped frequency dwells, which make partial ionograms. Scheduled inactive
hours, startup catch-up, offline/once commissioning, weak echoes, failed code
identification and missing-input errors alone do not count as capacity losses.
PRN resumption does not reprocess or count already-completed frequency dwells.
Recent events survive worker restarts and expire naturally.

CPU attribution uses processing-runtime evidence: process CPU runtime,
Linux scheduler run-queue delay, or observed cgroup CPU throttling during a
processing step that exceeded its schedule budget. This is a diagnostic of
CPU/scheduling pressure, not proof that it was the exclusive cause. Backlog
without that evidence is explicitly labelled CPU cause unconfirmed. Overall
CPU percent alone never creates an ionogram-loss alert. Global chirp-search
blocks are background discovery and are not counted as missed ionograms.

The GUI highlights affected station tabs and its red Needs attention button,
including from other station views and the propagation map. Details identify
the worker, loss units/counts and CPU evidence. Archived views and stale reports
do not trigger live loss warnings. Missing counters mean monitoring has not
reported yet, rather than an assertion that no data were lost.

Build receiver-compatible iono-receiver, iono-prn, iono-verso and iono-plots.
Run `scripts/install-processing-health.sh DIRECTORY` as the receiver user.
It saves rollback binaries and restarts only running ionogram/plot workers
using those executables. It preserves recorder/UHD, background search,
configuration, timing, CPU budgets and existing transport settings. DOB transfers
must stay at or below 16 KiB/s. TGO requires its native older-glibc build.
