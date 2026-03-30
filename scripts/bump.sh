#!/usr/bin/env bash
set -euo pipefail

LAST_TAG=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
RANGE=${LAST_TAG:+$LAST_TAG..HEAD}

SUBJECTS=$(git log $RANGE --pretty=format:%s)
FULL=$(git log $RANGE --pretty=format:%B)

if echo "$FULL" | grep -q 'BREAKING CHANGE' || echo "$SUBJECTS" | grep -qE '^[a-z]+(\(.+\))?!:'; then
  echo "major"
  exit 0
fi

if echo "$SUBJECTS" | grep -qE '^feat(\(.+\))?:'; then
  echo "minor"
  exit 0
fi

if echo "$SUBJECTS" | grep -qE '^(fix|perf)(\(.+\))?:'; then
  echo "patch"
  exit 0
fi

echo "none"