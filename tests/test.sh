#!/bin/sh
# majra test runner
# Usage: sh tests/test.sh [path-to-cc2]

CC="${1:-cc2}"
BUILD="build"
mkdir -p "$BUILD"

echo "=== majra test suite ==="

echo "--- core tests ---"
cat src/main.cyr | "$CC" 2>/dev/null > "$BUILD/majra" && chmod +x "$BUILD/majra" && "$BUILD/majra"
CORE=$?

echo ""
echo "--- expanded tests ---"
cat tests/test_core.cyr | "$CC" 2>/dev/null > "$BUILD/test_core" && chmod +x "$BUILD/test_core" && "$BUILD/test_core"
EXPANDED=$?

echo ""
echo "--- backend tests ---"
cat tests/test_backends.cyr | "$CC" 2>/dev/null > "$BUILD/test_backends" && chmod +x "$BUILD/test_backends" && "$BUILD/test_backends"
BACKEND=$?

echo ""
echo "=== Results ==="
[ $CORE -eq 0 ] && echo "  core:     PASS" || echo "  core:     FAIL ($CORE)"
[ $EXPANDED -eq 0 ] && echo "  expanded: PASS" || echo "  expanded: FAIL ($EXPANDED)"
[ $BACKEND -eq 0 ] && echo "  backend:  PASS" || echo "  backend:  FAIL ($BACKEND)"

echo ""
echo "--- benchmarks ---"
cat benches/bench_all.cyr | "$CC" 2>/dev/null > "$BUILD/bench_all" && chmod +x "$BUILD/bench_all" && "$BUILD/bench_all"

exit $(( CORE + EXPANDED ))
