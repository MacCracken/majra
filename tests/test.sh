#!/bin/sh
# majra test runner — the four offline suites, then the benches.
# Usage: sh tests/test.sh   (after `cyrius lib sync --full && cyrius deps`)
# --no-deps as in CI: a build that re-resolves deps perturbs ./lib/'s include order.

BUILD="build"
mkdir -p "$BUILD"

echo "=== majra test suite ==="

echo "--- core tests ---"
cyrius build --no-deps src/main.cyr "$BUILD/majra" && "$BUILD/majra"
CORE=$?

echo ""
echo "--- expanded tests ---"
cyrius build --no-deps tests/test_core.tcyr "$BUILD/test_core" && "$BUILD/test_core"
EXPANDED=$?

echo ""
echo "--- backend tests ---"
cyrius build --no-deps tests/test_backends.tcyr "$BUILD/test_backends" && "$BUILD/test_backends"
BACKEND=$?

echo ""
echo "--- patra-queue tests ---"
cyrius build --no-deps tests/test_patra_queue.tcyr "$BUILD/test_patra_queue" && "$BUILD/test_patra_queue"
PATRA=$?

echo ""
echo "=== Results ==="
[ $CORE -eq 0 ] && echo "  core:        PASS" || echo "  core:        FAIL ($CORE)"
[ $EXPANDED -eq 0 ] && echo "  expanded:    PASS" || echo "  expanded:    FAIL ($EXPANDED)"
[ $BACKEND -eq 0 ] && echo "  backend:     PASS" || echo "  backend:     FAIL ($BACKEND)"
[ $PATRA -eq 0 ] && echo "  patra-queue: PASS" || echo "  patra-queue: FAIL ($PATRA)"

echo ""
echo "--- benchmarks ---"
cyrius bench benches/bench_all.bcyr

[ $CORE -eq 0 ] && [ $EXPANDED -eq 0 ] && [ $BACKEND -eq 0 ] && [ $PATRA -eq 0 ]
