#!/usr/bin/env python3
"""Shared chirpsounder2 software version metadata."""

from __future__ import annotations

import os
import subprocess
from functools import lru_cache

__version__ = "0.2.0"


@lru_cache(maxsize=1)
def git_metadata() -> dict[str, str | bool | None]:
    repo_dir = os.path.dirname(os.path.abspath(__file__))
    try:
        commit = subprocess.check_output(
            ["git", "-C", repo_dir, "rev-parse", "--short=12", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
            timeout=2.0,
        ).strip()
        dirty = bool(
            subprocess.check_output(
                ["git", "-C", repo_dir, "status", "--porcelain"],
                text=True,
                stderr=subprocess.DEVNULL,
                timeout=2.0,
            ).strip()
        )
    except Exception:
        commit = None
        dirty = None
    return {"git_commit": commit, "git_dirty": dirty}


def software_metadata() -> dict[str, str | bool | None]:
    metadata = {"chirpsounder2_version": __version__}
    metadata.update(git_metadata())
    return metadata


def tag_hdf5(handle) -> None:
    for key, value in software_metadata().items():
        if value is None:
            continue
        handle.attrs[key] = value
