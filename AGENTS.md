# Codex Instructions

- Always develop locally in this repository first.
- Do not deploy code to a remote server by copying files directly with `scp` or similar ad hoc sync commands.
- Deployment workflow for any remote server is: make local changes, commit locally, push to the git repository, then pull/update the repository on the remote server.
- Preserve unrelated local or server-side runtime files, logs, build artifacts, and user changes unless explicitly asked to clean them up.
- When checking whether local and a remote server are in sync, compare git commits and tracked source changes separately from runtime logs and generated artifacts.
- W2NAF data products are stored on the server under `/home/hamsci/data/chirpsounder2`; the live Digital RF ringbuffer is `/mnt/ramdisk/hf25`.
- To make a bounded W2NAF detection plot, use `python3 plot_detectionfiles.py --config examples/marieluise/w2naf.ini --start YYYY-MM-DDTHH:MM:SS --end YYYY-MM-DDTHH:MM:SS --min-detections 10 --output /tmp/w2naf-detections.png`. `--start` and `--end` are UTC; `YYYY-MM-DD` is also accepted for whole-day plotting.
