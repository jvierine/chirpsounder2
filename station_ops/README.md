# Active station maintenance

These utilities remain in use; they are not part of the retired Python DSP archive.

- `iono_housekeeping.py` removes expired ring-buffer files and old parameter artifacts.
- `station_monitor.py` writes/uploads station health metadata for the GUI.
- `chirp_config.py` reads their compatibility INI settings.
- `chirpsounder_version.py` supplies software metadata.
- `propagation.py` is used by offline station configuration tools, not RF processing.

There is no RF signal processing or Matplotlib plotting in this directory.
Housekeeping's unused legacy status-plot implementation is retained only in
`python/`. The live station monitor supplies health data instead.

Launch from the repository root with the receiver's existing environment:

```sh
python3 station_ops/iono_housekeeping.py --config examples/marieluise/dombas.ini
python3 station_ops/station_monitor.py --config examples/marieluise/dombas.ini
```

These are station commands; local development should use
`conda run -n base python ...`. Runtime dependencies are listed in
`station_ops/requirements.txt`. Moving these files in Git does not restart
recorders or alter already running station processes.
