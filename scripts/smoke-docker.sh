#!/usr/bin/env bash
# Runs a built image for real and checks the things that matter for a self-hosted install:
# it starts locked in first-run setup, accepts the one-time code once, keeps the password, data and
# sign-ins across a restart, can change and reset the password, and runs as non-root.
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

get() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
post() { # post <path> <json> [curl args...]
  local path=$1 body=$2; shift 2
  curl -s -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d "$body" "$@" "$base$path"
}
setup_code() { docker logs "$name" 2>&1 | sed -n 's/.*Setup code: //p' | tail -1; }

# 1. First run: no password configured, so the server must start in setup mode.
docker run -d --name "$name" -p "127.0.0.1:$port:3000" "$image" >/dev/null
wait_healthy
ok "container is healthy with no password configured (its own --healthcheck passed)"

[ "$(docker inspect -f '{{.Config.User}}' "$name")" = "nonroot" ] || fail "container does not run as nonroot"
ok "runs as non-root"

curl -fsS "$base/" | grep -q "<title>Ledgerhold</title>" || fail "the web UI is not served"
ok "serves the web UI"

curl -fsS "$base/api/session" | grep -q '"setupRequired":true' || fail "not in setup mode on first run"
[ "$(post /api/rpc/overview '{}')" = "401" ] || fail "data was reachable before a password existed"
[ "$(post /api/login '{"password":"anything at all"}')" = "409" ] || fail "login was possible before setup"
ok "first run is locked: setup mode, no data, no login"

code=$(setup_code)
[ -n "$code" ] || fail "no setup code in the container log"
[ "$(post /api/setup "{\"code\":\"AAAA-AAAA-AAAA\",\"password\":\"$password\"}")" = "401" ] || fail "a wrong setup code was accepted"
[ "$(post /api/setup "{\"code\":\"$code\",\"password\":\"short\"}")" = "400" ] || fail "a too-short password was accepted"
[ "$(post /api/setup "{\"code\":\"$code\",\"password\":\"$password\"}" -c "$jar")" = "204" ] || fail "setup with the right code failed"
[ "$(post /api/setup "{\"code\":\"$code\",\"password\":\"another-password\"}")" = "409" ] || fail "the setup code worked twice"
ok "setup: wrong code and weak password refused, right code accepted once"

rpc add_person '{"name":"Alex"}' | grep -q '"name":"Alex"' || fail "could not add a person after setup"
ok "signed in straight after setup, data can be written"
stored=$(docker cp "$name:/data/password.hash" - | tar -xO) || fail "the password was not persisted"
case "$stored" in '$argon2'*) ;; *) fail "stored password is not an argon2 hash" ;; esac
case "$stored" in *"$password"*) fail "password stored in plain text" ;; esac
ok "password persisted as a hash"
loose=$(docker cp "$name:/data" - | tar -tvf - | awk '$1 !~ /^[-d]rw[x-]------/ {print}') || true
[ -z "$loose" ] || fail "files in /data readable by others: $loose"
ok "everything in /data is owner-only"

# 2. Restart: the password, the sign-in and the data all survive; there is no second setup.
docker restart "$name" >/dev/null
wait_healthy
rpc overview '{}' | grep -q '"name":"Alex"' || fail "data (or the sign-in cookie) did not survive a restart"
curl -fsS "$base/api/session" | grep -q '"setupRequired":false' || fail "setup mode came back after a restart"
[ -z "$(docker logs "$name" 2>&1 | sed -n 's/.*Setup code: //p' | tail -n +2)" ] || fail "a second setup code was printed"
[ "$(post /api/login '{"password":"wrong password"}')" = "401" ] || fail "wrong password accepted after restart"
[ "$(post /api/login "{\"password\":\"$password\"}")" = "204" ] || fail "right password rejected after restart"
ok "password, data and sessions survive a restart"

# 3. Change the password from the app.
[ "$(post /api/password "{\"current\":\"nope nope nope\",\"newPassword\":\"a brand new pass\"}" -b "$jar")" = "401" ] || fail "changed the password without the current one"
[ "$(post /api/password "{\"current\":\"$password\",\"newPassword\":\"a brand new pass\"}" -b "$jar")" = "204" ] || fail "could not change the password"
[ "$(post /api/rpc/overview '{}' -b "$jar")" = "401" ] || fail "the old session survived a password change"
[ "$(post /api/login '{"password":"a brand new pass"}')" = "204" ] || fail "the new password does not work"
ok "password change signs old sessions out"

# 4. Forgotten password: reset, restart, set up again. The data stays.
docker exec "$name" /usr/local/bin/ledgerhold-server reset-password | grep -q "Password forgotten" || fail "reset-password did nothing"
docker restart "$name" >/dev/null
wait_healthy
curl -fsS "$base/api/session" | grep -q '"setupRequired":true' || fail "not back in setup mode after reset-password"
code=$(setup_code)
rm -f "$jar"
[ "$(post /api/setup "{\"code\":\"$code\",\"password\":\"$password\"}" -c "$jar")" = "204" ] || fail "setup after a reset failed"
rpc overview '{}' | grep -q '"name":"Alex"' || fail "data was lost by a password reset"
ok "reset-password returns to setup and keeps the data"

# 5. A password in the environment skips setup entirely.
docker rm -f "$name" >/dev/null
docker run -d --name "$name" -p "127.0.0.1:$port:3000" -e LEDGERHOLD_PASSWORD="$password" "$image" >/dev/null
wait_healthy
curl -fsS "$base/api/session" | grep -q '"setupRequired":false' || fail "setup mode with LEDGERHOLD_PASSWORD set"
[ "$(post /api/login "{\"password\":\"$password\"}")" = "204" ] || fail "LEDGERHOLD_PASSWORD did not work"
ok "LEDGERHOLD_PASSWORD still works and skips setup"
echo "docker smoke tests passed"
