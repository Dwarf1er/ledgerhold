#!/usr/bin/env bash
# Runs a built image for real and checks the things that matter for a self-hosted install:
# it starts, becomes healthy, demands the password, keeps data and sign-ins across a restart,
# runs as non-root, and refuses to start on a network without a password.
# Usage: scripts/smoke-docker.sh [image] [host-port]
set -euo pipefail

image=${1:-ledgerhold:local}
port=${2:-3999}
name="ledgerhold-smoke-$$"
password="smoke-test-password"
base="http://127.0.0.1:$port"
jar=$(mktemp)

cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; rm -f "$jar"; }
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; docker logs "$name" 2>&1 | tail -20 >&2 || true; exit 1; }
ok() { echo "ok: $*"; }

wait_healthy() {
  for _ in $(seq 1 60); do
    [ "$(docker inspect -f '{{.State.Health.Status}}' "$name")" = "healthy" ] && return 0
    sleep 1
  done
  fail "container did not become healthy within 60s"
}

rpc() { curl -fsS -b "$jar" -H 'content-type: application/json' -d "${2-}" "$base/api/rpc/$1"; }

docker run -d --name "$name" -p "127.0.0.1:$port:3000" -e LEDGERHOLD_PASSWORD="$password" "$image" >/dev/null
wait_healthy
ok "container is healthy (its own --healthcheck passed)"

[ "$(docker inspect -f '{{.Config.User}}' "$name")" = "nonroot" ] || fail "container does not run as nonroot"
ok "runs as non-root"

curl -fsS "$base/" | grep -q "<title>Ledgerhold</title>" || fail "the web UI is not served"
ok "serves the web UI"

[ "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{}' "$base/api/rpc/overview")" = "401" ] \
  || fail "API answered without signing in"
ok "API requires a sign-in"

[ "$(curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{"password":"wrong"}' "$base/api/login")" = "401" ] \
  || fail "wrong password was accepted"
curl -fsS -c "$jar" -X POST -H 'content-type: application/json' -d "{\"password\":\"$password\"}" "$base/api/login" >/dev/null
rpc add_person '{"name":"Alex"}' | grep -q '"name":"Alex"' || fail "could not add a person"
ok "login works and data can be written"

docker restart "$name" >/dev/null
wait_healthy
rpc overview '{}' | grep -q '"name":"Alex"' || fail "data (or the sign-in cookie) did not survive a restart"
ok "data and the session survive a restart"

# A server reachable from the network without a password must refuse to start.
if docker run --rm -e LEDGERHOLD_PASSWORD= "$image" >/dev/null 2>"$jar.err"; then
  fail "started on 0.0.0.0 without a password"
fi
grep -q "LEDGERHOLD_PASSWORD" "$jar.err" || fail "unhelpful error when the password is missing: $(cat "$jar.err")"
rm -f "$jar.err"
ok "refuses to start without a password"
echo "docker smoke tests passed"
