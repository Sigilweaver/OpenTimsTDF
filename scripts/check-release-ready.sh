#!/usr/bin/env bash
# Checks that CI and the dependency audit are both green for a commit before
# it gets tagged for release. GitHub Actions has no way for publish.yml to
# `needs:` a job defined in ci.yml/audit.yml (separate workflow files can't
# depend on each other), so this has to be run by hand before `git tag`.
#
# Usage: scripts/check-release-ready.sh [ref]
#   ref defaults to HEAD.

set -euo pipefail

REF="${1:-HEAD}"

SHA="$(git rev-parse "$REF")"

check_workflow() {
    local workflow="$1"
    local run_json
    run_json="$(gh run list -w "$workflow" -c "$SHA" --json status,conclusion,url -L 1)"

    if [[ "$run_json" == "[]" ]]; then
        echo "FAIL: no run of $workflow found for commit $SHA"
        return 1
    fi

    local status conclusion url
    status="$(echo "$run_json" | jq -r '.[0].status')"
    conclusion="$(echo "$run_json" | jq -r '.[0].conclusion')"
    url="$(echo "$run_json" | jq -r '.[0].url')"

    if [[ "$status" != "completed" ]]; then
        echo "FAIL: latest $workflow run for $SHA is not completed (status: $status) - $url"
        return 1
    fi

    if [[ "$conclusion" != "success" ]]; then
        echo "FAIL: latest $workflow run for $SHA did not succeed (conclusion: $conclusion) - $url"
        return 1
    fi

    echo "OK: $workflow succeeded for $SHA - $url"
    return 0
}

ci_ok=0
audit_ok=0

check_workflow "ci.yml" || ci_ok=1
check_workflow "audit.yml" || audit_ok=1

if [[ "$ci_ok" -ne 0 || "$audit_ok" -ne 0 ]]; then
    exit 1
fi

echo "Release ready: ci.yml and audit.yml both succeeded for $SHA"
exit 0
