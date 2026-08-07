#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

expected_version="$(tr -d '[:space:]' < scripts/codegraph-version)"
actual_version="$(codegraph --version)"
db_path=".codegraph/codegraph.db"

if [[ "$actual_version" != "$expected_version" ]]; then
  echo "CodeGraph version mismatch: expected ${expected_version}, got ${actual_version}." >&2
  exit 1
fi

if [[ ! -f "$db_path" ]]; then
  echo "Missing shared CodeGraph snapshot: ${db_path}." >&2
  exit 1
fi

if git check-ignore -q "$db_path"; then
  echo "Shared CodeGraph snapshot is ignored by Git: ${db_path}." >&2
  exit 1
fi

codegraph sync --quiet
status_json="$(codegraph status --json)"

CODEGRAPH_STATUS_JSON="$status_json" CODEGRAPH_EXPECTED_VERSION="$expected_version" python3 - <<'PY'
import json
import os
import sys

status = json.loads(os.environ["CODEGRAPH_STATUS_JSON"])
expected = os.environ["CODEGRAPH_EXPECTED_VERSION"]
pending = status.get("pendingChanges", {})
index = status.get("index", {})

problems = []
if not status.get("initialized"):
    problems.append("graph is not initialized")
if index.get("state") != "complete":
    problems.append(f"index state is {index.get('state')!r}, not 'complete'")
if index.get("builtWithVersion") != expected:
    problems.append(
        f"snapshot was built with {index.get('builtWithVersion')!r}, expected {expected!r}"
    )
if index.get("reindexRecommended"):
    problems.append("CodeGraph recommends a full re-index")
if any(pending.get(kind, 0) != 0 for kind in ("added", "modified", "removed")):
    problems.append(f"pending source changes remain: {pending}")

if problems:
    print("CodeGraph verification failed:", file=sys.stderr)
    for problem in problems:
        print(f"- {problem}", file=sys.stderr)
    sys.exit(1)

print(
    "CodeGraph ready: "
    f"{status.get('fileCount', 0)} files, "
    f"{status.get('nodeCount', 0)} nodes, "
    f"{status.get('edgeCount', 0)} edges."
)
PY

integrity_output="$(sqlite3 "$db_path" 'PRAGMA wal_checkpoint(TRUNCATE); PRAGMA integrity_check;')"
integrity_result="$(printf '%s\n' "$integrity_output" | tail -n 1)"
if [[ "$integrity_result" != "ok" ]]; then
  echo "CodeGraph SQLite integrity check failed: ${integrity_result}" >&2
  exit 1
fi

echo "CodeGraph SQLite integrity: ok."
