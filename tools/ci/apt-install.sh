#!/usr/bin/env bash
# apt-get install with a deadline and retries (catalog dev/v1 PLAN.md CI1): runner mirrors fail to resolve or hang
# at times, and one attempt failed whole runs (Link e2e 2026-11-11: "Temporary failure resolving").
#   tools/ci/apt-install.sh PACKAGE...
set -u
for attempt in 1 2 3; do
  timeout 300 sudo apt-get update -o Acquire::Retries=3 &&
    timeout 900 sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -o Acquire::Retries=3 "$@" && exit 0
  echo "apt attempt $attempt failed; retrying" >&2
  sleep $((attempt * 20))
done
exit 1
