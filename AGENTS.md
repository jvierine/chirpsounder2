# Codex Instructions

- Always develop locally in this repository first.
- Do not deploy code to a remote server by copying files directly with `scp` or similar ad hoc sync commands.
- Deployment workflow for any remote server is: make local changes, commit locally, push to the git repository, then pull/update the repository on the remote server.
- Preserve unrelated local or server-side runtime files, logs, build artifacts, and user changes unless explicitly asked to clean them up.
- When checking whether local and a remote server are in sync, compare git commits and tracked source changes separately from runtime logs and generated artifacts.
