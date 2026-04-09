#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "Usage: $0 <new-version>"
    echo "Example: $0 2.1.0"
    exit 1
fi

NEW_VERSION="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Bumping majra to version ${NEW_VERSION}..."

echo "$NEW_VERSION" > "$REPO_ROOT/VERSION"
echo "  Updated VERSION"

sed -i "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" "$REPO_ROOT/cyrius.toml"
echo "  Updated cyrius.toml"

FILE_VERSION=$(cat "$REPO_ROOT/VERSION" | tr -d '[:space:]')
CYRIUS_VERSION=$(grep '^version = ' "$REPO_ROOT/cyrius.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')

if [ "$FILE_VERSION" != "$NEW_VERSION" ] || [ "$CYRIUS_VERSION" != "$NEW_VERSION" ]; then
    echo "ERROR: Version mismatch after bump"
    exit 1
fi

echo ""
echo "Version bumped to ${NEW_VERSION}"
echo ""
echo "Next steps:"
echo "  git add VERSION cyrius.toml"
echo "  git commit -m \"bump to ${NEW_VERSION}\""
echo "  git tag ${NEW_VERSION}"
echo "  git push && git push --tags"
