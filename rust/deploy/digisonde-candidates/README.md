# Additional Digisonde commissioning

`catalog.json` records GIRO program metadata, its actual latest archive time,
receiver targets and approximate great-circle delays. This is a disabled
commissioning catalog, not a set of active production station configurations.
It does not change the existing verified W2NAF Millstone receiver. The Thule
entries contain native receiver probe configurations for bounded local tests;
outputs go to `/tmp/iono-thule-commission`, outside science publication.

Targets are Thule, Dourbes and Chilton at TGO/DOB/KHO, and Thule, Millstone,
Wallops, Eglin, Boulder, Austin and Ramey at W2NAF. Wallops WP937 and WI937
are alternate archive identities for the locality, not evidence for two RF
transmitters. Positions are approximate GIRO metadata, not surveyed antennas.

On 2026-11-06 GIRO supplied current Dourbes and Millstone records and Thule
records from earlier the same day. Eglin's latest record was November 1.
Chilton, Wallops, Boulder, Austin and Ramey had historical records only.
Missing recent submissions do not establish that a transmitter is off.

Dourbes has a five-minute sounding cadence, with separate extended quarter-hour
and night programs. Its day sweep starts at 1.5 MHz, not the legacy receiver's
1 MHz, and extends to 15 MHz. The quarter-hour day program extends to 23 MHz.
Its 100 Hz physical pulse rate and two fine frequencies must be distinguished
from the effective per-carrier recurrence. The legacy 15-minute, 1–12 MHz
configuration is insufficient. Chilton's historical header reports 200 Hz,
not the legacy 100 Hz assumption; frequency search and current onset still
need on-air verification. Do not infer pulse ordering, coherent dwell or a
native decoder mode from the GIRO integer phase-code field alone.

The native commissioning verifier accepts `ipp_candidates_us` containing
5000, 10000, 20000 and 40000. The latter values test effective recurrence under
multiplexing, not changed physical pulse rates. Synthetic tests recover both
20 and 40 ms spacings. A DOB 208-window scan found isolated weak matches but
no independently identified, consecutive Dourbes sweep. No continuous worker
or transmitter identity was enabled from those matches.

For a bounded Thule test, copy the relevant `probe_receiver_config` object to a
receiver-local JSON file. Use the existing native `iono-receiver` binary built
for that host and a future epoch on the 450-second UTC grid:

```sh
systemd-run --user --unit=iono-commission-thule \
  --property=Nice=15 --property=CPUQuota=50% \
  --property=MemoryMax=512M --property=RuntimeMaxSec=300 \
  "$HOME/bin/iono-receiver" digisonde /tmp/thule-probe.json \
  --once --start UNIX_EPOCH
```

This is one finite observation, not a boot-enabled receiver. Keep outputs out
of the public archive until consecutive stepped-frequency echoes agree with
the schedule and code/polarization/frequency sequence. Establish transmit
group delay before treating apparent range as a calibrated radio path. Only
then promote a reviewed JSON to `~/.config/chirpsounder2/`, install the existing
`iono-digisonde@.service`, and enable its instance. Measure CPU-time deltas and
missed voltage windows first. Acquisition, Ramfjordmoen, SGO scheduled LFM and
global chirp detection retain their established priority; these tests run below
all of them. Avoid duplicating an already running legacy sounder receiver.

Metadata source: [GIRO DIDBase](https://giro.uml.edu/didbase/), authorized
read-only queries of LOCATION, MEASUREMENT and PROGRAM. Credentials are not
included. Interpret program fields using the official Digisonde documentation
and validate against actual voltage. GIRO data usage follows its
[Rules of the Road](https://giro.uml.edu/didbase/RulesOfTheRoad.html).

## First Thule probe outcome

At 12:52:30 UTC on November 6, finite probes completed on TGO, KHO and W2NAF
with sparse files of roughly 27, 35 and 78 KiB respectively. The Nordic plots
lack a continuous independently identified Thule trace. W2NAF has a strong
return, but comparison against its simultaneous Millstone ionogram shows the
5.5 and 6.0 MHz echoes shifted by exactly 5995.8 km: two 10 ms vacuum pulse
periods. This is not independent Thule evidence. No new continuous worker or
public Thule product was enabled.

DOB's old unused receiver binary logged a hard-coded Ramfjordmoen name while
writing the correct THJ76 transmitter metadata. It also emitted dense SNR and
ignored the newer commissioning range-reference text. Its test file was kept
outside the science archive. Use the reviewed native binary and verify its
metadata/sparse output before commissioning further candidates. The RF/DSP
replacement binary is from public commit 518b38c (SHA-256
4521dab51eb4d5a553b7e0a510b25cfd43e41272209086e3f49bc1e575aa08f2).

The initial W2NAF test timed out because its five-minute runtime allowance
included waiting for the future epoch. A retry against retained local voltage
completed. For future starts, launch the finite service at the epoch with a
systemd timer, or set the runtime allowance to include the scheduled wait.
Do not increase a CPU ceiling to compensate for a scheduling mistake.
