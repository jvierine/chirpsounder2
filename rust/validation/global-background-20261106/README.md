# Global chirp-search background policy, 6 November 2026

User policy: global search is the lowest-priority task, targeting one logical
CPU while reception and scheduled processing take precedence. The aggregate
CPUQuota=100% budget applies to the service including all threads, not one core
per worker. Nice=19 and CPUWeight=1/CPUShares=2 make it yield under contention.
This is a budget, not a hard guaranteed CPU reservation under arbitrary load.
No acquisition worker was restarted or put in the detector cgroup.

Applied live and persistently to KHO, DOB and TGO. Their detector PIDs remained
1726315, 1508996 and 2547747, respectively. KHO measured 99.3% process CPU
(one logical CPU), whole-host idle 51.0%, versus 2–3.5% before the maintenance
and detector-budget changes. Data gaps remain and are not declared fixed.

W2NAF previously had two legacy MPI detectors and a separate legacy metadata
consolidator inside the acquisition service. The tested Rust detector was
installed separately as iono-receiver-global, leaving its Digisonde binary
unchanged. A native --once live-ring check succeeded and published HDF5.
Only MPI detection and its metadata consolidator were stopped. The new native
iono-rust-detect service is enabled and budgeted independently; the station
startup patch starts it on reboot. Acquisition PID 3582315 stayed running.

Native binary SHA256:
c3d88865b1f1719ae021cefc441d0ff733cb50b63217e4ab15a991b9a13214ab
Source DSP revision: 072b713; maintenance-only updates followed it.

KREA's SSH tunnel on port 22220 was absent on juha.no. Its tracked detector
unit has the same policy ready for deployment, but the live node could not be
changed or verified. No claim of completed KREA deployment is made.

The reusable installer and station-priority maintenance script are updated,
as are the default, Nordic and KREA detector unit templates. Bash syntax
checks passed. The W2NAF startup patch retains other station tasks; it does
not claim a complete migration of that station's remaining legacy workers.
