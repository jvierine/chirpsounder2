#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
BRANCH="${BRANCH:-master}"

if [[ -n "$(git -C "${REPO_DIR}" status --porcelain)" ]]; then
    echo "Working tree is not clean. Commit, stash, or discard local changes before deploying." >&2
    git -C "${REPO_DIR}" status --short
    exit 1
fi

echo "Fetching origin/${BRANCH}"
git -C "${REPO_DIR}" fetch origin "${BRANCH}"

echo "Updating ${REPO_DIR} to origin/${BRANCH}"
git -C "${REPO_DIR}" pull --ff-only origin "${BRANCH}"

echo "Deploying web files"
"${SCRIPT_DIR}/deploy_web.sh"

echo "Web deployment completed from origin/${BRANCH}"
