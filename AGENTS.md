# Codex Instructions

- Work locally in this repository first.
- Do not deploy code to `w2naf.com` by copying files directly with `scp` or similar ad hoc sync commands.
- Deployment workflow for `w2naf.com` is: make local changes, commit locally, push to git, then pull/update the repository on `w2naf.com`.
- Preserve unrelated local or server-side runtime files, logs, build artifacts, and user changes unless explicitly asked to clean them up.
- When checking whether local and `w2naf.com` are in sync, compare git commits and tracked source changes separately from runtime logs and generated artifacts.
