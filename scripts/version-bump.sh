#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 2.3.0"
    exit 1
fi

NEW_VERSION="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Bumping majra to version ${NEW_VERSION}..."

echo "$NEW_VERSION" > "$REPO_ROOT/VERSION"
echo "  Updated VERSION"

# cyrius.cyml uses ${file:VERSION} — nothing to sed in the manifest.
# Verify the file-ref is still present.
if ! grep -q '${file:VERSION}' "$REPO_ROOT/cyrius.cyml"; then
    echo "ERROR: cyrius.cyml no longer uses \${file:VERSION} — manual sync required"
    exit 1
fi

FILE_VERSION=$(tr -d '[:space:]' < "$REPO_ROOT/VERSION")
if [ "$FILE_VERSION" != "$NEW_VERSION" ]; then
    echo "ERROR: VERSION mismatch after bump"
    exit 1
fi

echo ""
echo "Version bumped to ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  1. Regenerate bundles:  cyrius distlib && cyrius distlib backends"
echo "  2. Update CHANGELOG.md with [${NEW_VERSION}] entry"
echo "  3. Commit:              git add VERSION CHANGELOG.md dist/ && git commit -m \"bump to ${NEW_VERSION}\""
echo "  4. Tag + push:          git tag ${NEW_VERSION} && git push && git push --tags"
