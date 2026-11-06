# Legacy Python code — retired

Everything in this directory belongs to the retired Python implementation.
It is **legacy code, no longer used by the supported Rust reception and
plotting pipeline**. It is retained for historical reference and offline
comparison, not for installation or station operation.

This includes the Digisonde and LFM receivers, chirp discovery, ionogram/RTI/RF
plotting, old web publishing, their Python dependencies, the ctypes C helper,
and old launcher/service examples. Do not start these workers on migrated
stations. No current native service loads Python DSP or Matplotlib from here.

Use [the Rust implementation](../rust/README.md) and
[reviewed station configurations](../rust/deploy/stations) instead.
[The historical README](README-historical.md) describes the old implementation;
its commands and paths are obsolete. Archived launchers are not runnable
recipes for the current tree. Some developer parity checks may explicitly read
these frozen reference algorithms offline; they are not station workers.

Active maintenance-only Python utilities live separately in
[station_ops/](../station_ops/README.md). Their existence does not make the
retired signal processors or plotters supported again.
