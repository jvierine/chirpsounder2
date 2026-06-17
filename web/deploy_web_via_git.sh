#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
WEB_SERVER="${WEB_SERVER:-j@juha.no}"
SERVER_REPO_DIR="${SERVER_REPO_DIR:-/home/j/src/chirpsounder2}"
BRANCH="${BRANCH:-$(git -C "${REPO_DIR}" branch --show-current)}"

if [[ -z "${BRANCH}" ]]; then
    echo "Could not determine the current git branch." >&2
    exit 1
fi

if [[ -n "$(git -C "${REPO_DIR}" status --porcelain)" ]]; then
    echo "Local working tree is not clean. Commit or stash changes before deploying." >&2
    git -C "${REPO_DIR}" status --short
    exit 1
fi

echo "Pushing ${BRANCH} to origin"
git -C "${REPO_DIR}" push origin "${BRANCH}"

echo "Pulling ${BRANCH} on ${WEB_SERVER}:${SERVER_REPO_DIR}"
ssh "${WEB_SERVER}" \
    "cd '${SERVER_REPO_DIR}' && BRANCH='${BRANCH}' ./web/update_web_from_git.sh"

echo "Deployment completed from git branch ${BRANCH}"
