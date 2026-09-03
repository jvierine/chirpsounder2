# PRN ionosonde schedules

This directory contains transmitter schedule files used to reproduce PRN
ionosonde timing and frequency programs from a wideband DigitalRF recording.
The `station_id` in each INI is the pseudorandom-code seed.

Receiver-site phase offsets and plot labels belong in the receiver station
configuration (for example, `examples/marieluise/kho.ini`) so one transmitter
schedule can be reused at multiple receiving stations.

Current schedules:

- `unis_lyr_cw.ini`: UNIS Longyearbyen, seed 4, 60-second sweep.
- `irf_prn_kiruna_2min.ini`: IRF Kiruna KI167, 120-second sweep.
- `irf_prn3_uppsala.ini`: IRF Uppsala UP158 frequency program. KHO applies
  the required UTC phase offset in its station configuration.
