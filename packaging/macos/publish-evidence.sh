#!/usr/bin/env bash
# publish-evidence.sh — put one job's logs where a Contents-scoped reader can
# see them: the `macos-status` branch, under `run-<run_id>/<stage>/`.
#
# The Actions API is not reachable for the packaging agents, so branch contents
# are the evidence channel (same pattern as ci-status / ci-evidence-<os>).
#
# USAGE: publish-evidence.sh <stage> <job-status> [log files...]
# Env:   GITHUB_RUN_ID, GITHUB_SHA, GITHUB_SERVER_URL, GITHUB_REPOSITORY
set -euo pipefail

stage="${1:?stage}"; status="${2:?job status}"; shift 2 || true
run="${GITHUB_RUN_ID:?GITHUB_RUN_ID}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$repo_root"

# Stage the payload outside the work tree first: the checkout of the evidence
# branch replaces the tree we were building in.
stage_dir="$(mktemp -d)"
printf '{\n  "run_id": %s,\n  "sha": "%s",\n  "stage": "%s",\n  "job_status": "%s",\n  "run_url": "%s/%s/actions/runs/%s",\n  "published_at": "%s"\n}\n' \
  "$run" "${GITHUB_SHA:-unknown}" "$stage" "$status" \
  "${GITHUB_SERVER_URL:-https://github.com}" "${GITHUB_REPOSITORY:-}" "$run" \
  "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$stage_dir/status.json"
for log in "$@"; do
  if [ -f "$log" ]; then cp "$log" "$stage_dir/$(basename "$log")"; fi
done
if [ -d dist ]; then
  ( cd dist && find . -type f -exec shasum -a 256 {} \; ) > "$stage_dir/artifacts.sha256" || true
  ( cd dist && find . -type f -exec ls -l {} \; ) > "$stage_dir/artifacts.ls" || true
else
  # `dist` is where the pack script writes; make the absence explicit rather
  # than silent, so a stage that produced nothing is visible in the evidence.
  echo "no dist/ directory in this stage" > "$stage_dir/artifacts.absent"
fi

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git fetch -q origin macos-status:refs/remotes/origin/macos-status 2>/dev/null || true
if git checkout -q -B macos-status origin/macos-status 2>/dev/null; then :; else
  echo "macos-status branch absent; starting a fresh history"
  git checkout -q -B macos-status
fi

dest="macos-status/run-${run}/${stage}"
mkdir -p "$dest"
cp "$stage_dir"/* "$dest"/ 2>/dev/null || true
git add -f macos-status
git commit -q -m "macos-status: run ${run} ${stage}=${status}" || echo "nothing to commit"
git push origin macos-status || { git pull --rebase -q origin macos-status && git push origin macos-status; }
echo "evidence published to macos-status/run-${run}/${stage}"
