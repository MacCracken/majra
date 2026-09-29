#!/bin/sh
# majra test runner
# Usage: sh tests/test.sh

BUILD="build"
mkdir -p "$BUILD"

echo "=== majra test suite ==="

echo "--- core tests ---"
cyrius build src/main.cyr "$BUILD/majra" && "$BUILD/majra"
CORE=$?

echo ""
echo "--- expanded tests ---"
cyrius build tests/test_core.tcyr "$BUILD/test_core" && "$BUILD/test_core"
EXPANDED=$?

echo ""
echo "--- backend tests ---"
cyrius build tests/test_backends.tcyr "$BUILD/test_backends" && "$BUILD/test_backends"
BACKEND=$?

echo ""
echo "=== Results ==="
[ $CORE -eq 0 ] && echo "  core:     PASS" || echo "  core:     FAIL ($CORE)"
[ $EXPANDED -eq 0 ] && echo "  expanded: PASS" || echo "  expanded: FAIL ($EXPANDED)"
[ $BACKEND -eq 0 ] && echo "  backend:  PASS" || echo "  backend:  FAIL ($BACKEND)"

echo ""
echo "--- benchmarks ---"
# 2.9.2: the benches are a .bcyr run by `cyrius bench`; this built a benches/bench_all.cyr that
# no longer exists and printed "FAILED" on every run (non-fatal, so it went unnoticed).
cyrius bench benches/bench_all.bcyr

exit $(( CORE + EXPANDED ))
