#!/usr/bin/env bash
# Read-only deployment guard. No keys, tokens, config bodies or transcript content are read.
set -euo pipefail
failures=0
fail() { printf 'FAIL: %s\n' "$*" >&2; failures=$((failures + 1)); }
info() { printf '%s\n' "$*"; }

[[ "$(uname -s)" == Linux ]] || fail "Deployment requires Linux."
actual_host=$(hostname -s)
if [[ -z "${EXPECTED_HOST:-}" ]]; then
  fail "Set EXPECTED_HOST from the verified VM inventory."
elif [[ "$actual_host" != "$EXPECTED_HOST" ]]; then
  fail "Hostname does not match EXPECTED_HOST."
fi
info "Observed hostname: $actual_host"

if command -v pveversion >/dev/null 2>&1 || [[ -d /etc/pve ]]; then
  fail "This is a Proxmox host. Select the dedicated application VM."
fi
if ! virtualization=$(systemd-detect-virt --vm 2>/dev/null); then
  fail "Could not verify that this is a VM."
else
  info "Virtualization: $virtualization"
fi
command -v apt-get >/dev/null 2>&1 || fail "The current installer requires an apt-based guest."
[[ "${REV:-}" =~ ^[0-9a-f]{40}$ ]] || fail "REV must be a full lowercase Git commit SHA."

listen=${LISTEN:-127.0.0.1:8787}
if [[ "$listen" =~ ^127\.0\.0\.1:([0-9]{1,5})$ ]]; then
  port=${BASH_REMATCH[1]}
  (( 10#$port >= 1 && 10#$port <= 65535 )) || fail "LISTEN port is outside 1..65535."
else
  fail "Ingest must listen on IPv4 loopback behind the reverse proxy."
fi

# Public exposure is a separate explicit step. Never replace an unrelated proxy config.
if [[ -n "${PUBLIC_HOSTS:-}" ]]; then
  for host in $PUBLIC_HOSTS; do
    [[ "$host" =~ ^[a-zA-Z0-9]([a-zA-Z0-9.-]*[a-zA-Z0-9])?$ ]] &&
      [[ "$host" == *.* && "$host" != *..* ]] ||
      fail "PUBLIC_HOSTS must contain only DNS hostnames."
  done
  if [[ -e /etc/caddy/Caddyfile ]] &&
     ! grep -qF '# rroflow.ai: written by deploy/tx-infra-prod/deploy.sh;' /etc/caddy/Caddyfile; then
    fail "Existing Caddyfile is not owned by this installer; integrate the vhost explicitly."
  fi
  if listeners=$(ss -ltnpH '( sport = :80 or sport = :443 )' 2>/dev/null); then
    other=$(printf '%s\n' "$listeners" | grep -v '"caddy"' | grep -v '^$' || true)
    [[ -z "$other" ]] || fail "Ports 80/443 are occupied by another or unidentified service."
  else
    fail "Could not inspect public listener ownership."
  fi
fi

if (( failures )); then
  printf 'Preflight refused deployment (%s check(s)). No changes made.\n' "$failures" >&2
  exit 1
fi
info "Preflight passed: identity, VM placement and listener guards only."
info "Capacity, backup/restore, authentication and release qualification still require evidence."
