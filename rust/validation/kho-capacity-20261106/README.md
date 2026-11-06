# KHO capacity check, 6 November 2026, 20:41–20:47 UTC

Live host obliono: i7-3770, four physical cores/eight logical CPUs, 16 GiB RAM.
It supports AVX and SSE4, but not AVX2/FMA. CPU thermal sensor was 60 C;
core throttle count was zero. The data filesystem is NTFS through ntfs-3g.

Before: total CPU busy 96–98%; global detector about 400% process CPU,
VERSO about 97%, acquisition 100%, housekeeping 52%, monitor 31%, and
ntfs-3g 64% in one interval. Process CPU percentages are per logical CPU;
do not add them as percentages of the whole computer.

The housekeeping archive scan ran every second, traversing day directories
back to May. It now runs once per 300 seconds while RAM-ring cleanup remains
every second, with unchanged file age thresholds. Expired parameter artifacts
can consequently remain up to another scan interval, plus traversal time.
The health freshness check visits root files and UTC day directories within
its freshness horizon, not all historical days. Digisonde and PRN outputs count.

Site-compatible patches are retained here: they preserve KHO's existing
DigitalRF metadata-recovery helper and monitor timestamp fixes. Both root-level
scripts were installed atomically with backups in the receiver's rollout
state directory. Only those two maintenance processes were restarted (nice 15).
The existing KHO startup script references these same files, preserving changes
across restart. Acquisition PID 1468893 and all RF workers were left running.

Deployed SHA256:
- iono_housekeeping.py: b8f167eb14f5c40371ce5e3e1b6df30d558a063d818547453146aca5b114bff4
- station_monitor.py: e1a000fdc740ef4d2575e59c831c8976b25a274d2f7470eeb1d8743baa383e6d

After: sampled CPU idle 8–15%, versus 2–3.5% before. Housekeeping about 1.8%
in the later interval; ntfs-3g varied from 1.5% between health checks to 26–31%
during a health check. These are short samples, not sustained capacity claims.
Health metadata uploads succeeded at 20:45:42 and 20:46:47 UTC. SGO products
were published at 20:44:40 and 20:46:39, but a 20:45 sounding was still missed.

Unresolved: acquisition logs still report packet discontinuities, and the global
chirp detector skips roughly 170–183 blocks per minute (4,194,304 samples per
block at 25 MHz). The scan fix does not establish uninterrupted acquisition or
continuous global-search coverage. No sounder or detection bank was disabled.
Further work should profile the detector and recorder independently; reducing
worker count alone trades global coverage for capacity. More modern hardware
would provide physical cores and AVX2/FMA unavailable on this CPU. Moving
storage to a native Linux filesystem needs a planned data migration, not a
live reformat.

Validation: three focused freshness tests passed (midnight/day rollover,
historical archive exclusion, root/current Digisonde compatibility). No RF DSP
or plotting code changed.
