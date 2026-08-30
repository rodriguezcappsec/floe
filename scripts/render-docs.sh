#!/bin/sh
set -eu

command -v cmark-gfm >/dev/null 2>&1 || {
    echo "cmark-gfm is required for the release documentation render gate" >&2
    exit 1
}

temporary=$(mktemp -d)
trap 'rm -rf -- "$temporary"' EXIT HUP INT TERM

for document in \
    README.md SECURITY.md CHANGELOG.md \
    docs/GETTING_STARTED.md docs/INSTALLATION.md docs/MIGRATIONS.md \
    docs/USER_GUIDE.md docs/ADMINISTRATION.md docs/ACCESSIBILITY.md \
    docs/RECOVERY.md docs/DEBUGGING.md docs/LOCALIZATION.md \
    docs/PERFORMANCE.md docs/ARCHITECTURE.md docs/DEVELOPMENT.md \
    docs/PRIVACY_SECURITY.md docs/PRIVILEGED_ACCESS.md \
    docs/FEATURE_MATRIX.md docs/ROADMAP.md
do
    output=$(printf '%s' "$document" | tr '/' '_')
    cmark-gfm --validate-utf8 --extension table "$document" > "$temporary/$output.html"
done

echo "phase-21c-render-ok"
