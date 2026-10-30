#!/usr/bin/env bash
# Build and install rroflow-ingest as a hardened systemd service on a Debian host, with its
# data in the host's Postgres. Approved 2026-10-26 for tx-infra-prod (rl07): localhost only.
#
# - Postgres: Debian's package, listening on localhost only (Debian default). The service runs
#   as the system user `rroflow` and reaches the database `rroflow` over the Unix socket with
#   peer authentication, so no database password exists anywhere.
# - Keys are minted on the host and never printed; only their SHA-256 hashes enter the config.
# - Re-running is safe: users, roles, database, keys and data are kept; the binary is rebuilt.
set -euo pipefail

REV="${REV:?commit to build}"
ROOT=/opt/rroflow
export RUSTUP_HOME=$ROOT/rust/rustup CARGO_HOME=$ROOT/rust/cargo
export PATH=$CARGO_HOME/bin:$PATH
LISTEN="${LISTEN:-127.0.0.1:8787}"
# Public hostnames Caddy serves with TLS, all proxied to $LISTEN: the API and the dashboard.
# Empty skips the proxy. Certificates are issued once each name's A record points here.
PUBLIC_HOSTS="${PUBLIC_HOSTS:-}"
# Nomen sign-in for this host (project `rroflow`, Shippin Platform org). Used when the
# ingest app's key file is present at $NOMEN_KEY_FILE.
NOMEN_ISSUER="${NOMEN_ISSUER:-}"
NOMEN_PROJECT="${NOMEN_PROJECT:-}"
NOMEN_CLIENTS="${NOMEN_CLIENTS:-}"
NOMEN_KEY_FILE=/etc/rroflow/nomen-key.json
BOOTSTRAP_DEVSPACE="${BOOTSTRAP_DEVSPACE:?set the intended initial devspace; no personal account is assumed}"
[[ "$BOOTSTRAP_DEVSPACE" =~ ^[A-Za-z0-9_-]{1,64}$ && "$BOOTSTRAP_DEVSPACE" != selftest ]] || {
  echo 'Invalid BOOTSTRAP_DEVSPACE (selftest is reserved for deployment verification).' >&2; exit 1;
}
# Fail before installing packages or changing services. Operator TOML is an
# alternative explicit configuration; no hosted issuer or registration is assumed.
if [[ ! -f /etc/rroflow/nomen.toml && -s "$NOMEN_KEY_FILE" ]]; then
  [[ "$NOMEN_ISSUER" =~ ^https://[A-Za-z0-9.-]+(:[0-9]{1,5})?$ &&
     "$NOMEN_PROJECT" =~ ^[A-Za-z0-9_-]+$ && -n "$NOMEN_CLIENTS" ]] || {
    echo 'Set NOMEN_ISSUER, NOMEN_PROJECT and NOMEN_CLIENTS explicitly, or supply nomen.toml.' >&2; exit 1;
  }
  for client in $NOMEN_CLIENTS; do
    [[ "$client" =~ ^[A-Za-z0-9_-]+$ ]] || { echo 'Invalid NOMEN_CLIENTS identifier.' >&2; exit 1; }
  done
fi

# Resolve from this pinned checkout; piping deploy.sh alone to bash is not supported.
SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
export REV LISTEN PUBLIC_HOSTS
bash "$SCRIPT_DIR/preflight.sh"

echo "== packages"
export DEBIAN_FRONTEND=noninteractive
# A host without a Proxmox subscription has enterprise repos that answer 401; apt-get update then
# exits non-zero although the Debian repos refreshed. Keep going and let the install fail loudly
# if the packages are really unavailable. Repository configuration is left as it is.
apt-get update -qq || echo "apt-get update reported errors (see above); continuing with the repos that refreshed"
apt-get install -y -qq build-essential pkg-config curl ca-certificates postgresql >/dev/null

echo "== postgres: role and database rroflow (peer auth over the Unix socket)"
systemctl enable --now postgresql
id -u rroflow >/dev/null 2>&1 || useradd --system --home-dir /var/lib/rroflow --no-create-home --shell /usr/sbin/nologin rroflow
pg() { runuser -u postgres -- psql -v ON_ERROR_STOP=1 -qAt "$@"; }
[ "$(pg -c "SELECT 1 FROM pg_roles WHERE rolname = 'rroflow'")" = 1 ] || pg -c "CREATE ROLE rroflow LOGIN"
[ "$(pg -c "SELECT 1 FROM pg_database WHERE datname = 'rroflow'")" = 1 ] || pg -c "CREATE DATABASE rroflow OWNER rroflow"
pg -c "SHOW listen_addresses"

echo "== rust toolchain in $ROOT/rust"
mkdir -p "$ROOT/rust"
if [ ! -x "$CARGO_HOME/bin/rustup" ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain none --no-modify-path
fi

echo "== source $REV"
rm -rf "$ROOT/src" && mkdir -p "$ROOT/src"
curl -sSfL "https://codeload.github.com/rrflow/rroflow-development/tar.gz/$REV" \
  | tar -xz -C "$ROOT/src" --strip-components=1
cd "$ROOT/src"

echo "== build (toolchain from rust-toolchain.toml)"
cargo build --release --locked -p rroflow-ingest
install -m 0755 target/release/rroflow-ingest /usr/local/bin/rroflow-ingest
/usr/local/bin/rroflow-ingest --version

echo "== keys and config"
install -d -m 0755 /etc/rroflow
install -d -m 0700 /etc/rroflow/keys
mint() { # mint <devspace>: writes the key to /etc/rroflow/keys/<devspace>.key, prints only the hash
  local out key
  out=$(/usr/local/bin/rroflow-ingest keygen --devspace "$1")
  key=$(printf '%s\n' "$out" | sed -n 's/^ingest key (shown once): //p')
  umask 077; printf '%s\n' "$key" > "/etc/rroflow/keys/$1.key"
  printf '%s\n' "$out" | sed -n 's/^key_sha256 = \["\(.*\)"\]$/\1/p'
}
[ -s "/etc/rroflow/keys/$BOOTSTRAP_DEVSPACE.key" ] || mint "$BOOTSTRAP_DEVSPACE" > "/etc/rroflow/keys/$BOOTSTRAP_DEVSPACE.sha256"
[ -s /etc/rroflow/keys/selftest.key ] || mint selftest > /etc/rroflow/keys/selftest.sha256
cat > /etc/rroflow/ingest.toml <<EOF
listen = "$LISTEN"
database_url = "postgres:///rroflow?host=/run/postgresql&user=rroflow"
db_max_connections = 10
max_batch = 500
max_body_bytes = 16777216

[[devspace]]
id = "$BOOTSTRAP_DEVSPACE"
key_sha256 = ["$(cat "/etc/rroflow/keys/$BOOTSTRAP_DEVSPACE.sha256")"]

[[devspace]]
id = "selftest"
key_sha256 = ["$(cat /etc/rroflow/keys/selftest.sha256)"]
EOF
# Nomen sign-in: on when the ingest app's key file is provisioned by the operator,
# never from the repo. /etc/rroflow/nomen.toml, when an operator writes one, replaces the
# values above. See docs/plan/identity-and-devspaces.md.
if [ -f /etc/rroflow/nomen.toml ]; then
  { echo; cat /etc/rroflow/nomen.toml; } >> /etc/rroflow/ingest.toml
  echo "nomen sign-in: on (operator's /etc/rroflow/nomen.toml)"
elif [ -s "$NOMEN_KEY_FILE" ]; then
  chown rroflow:rroflow "$NOMEN_KEY_FILE"; chmod 0600 "$NOMEN_KEY_FILE"
  clients=$(printf '"%s", ' $NOMEN_CLIENTS); clients="[${clients%, }]"
  cat >> /etc/rroflow/ingest.toml <<EOF

[nomen]
issuer = "$NOMEN_ISSUER"
audiences = ["$NOMEN_PROJECT"]
clients = $clients
key_file = "$NOMEN_KEY_FILE"
EOF
  echo "nomen sign-in: on ($NOMEN_ISSUER, project $NOMEN_PROJECT)"
else
  echo "nomen sign-in: off (no $NOMEN_KEY_FILE yet; keys still work)"
fi
chmod 0644 /etc/rroflow/ingest.toml

echo "== systemd unit"
cat > /etc/systemd/system/rroflow-ingest.service <<'EOF'
[Unit]
Description=rroflow.ai ingest (devspace turns and signals)
After=network-online.target postgresql.service
Wants=network-online.target
Requires=postgresql.service

[Service]
ExecStart=/usr/local/bin/rroflow-ingest serve --config /etc/rroflow/ingest.toml
User=rroflow
Group=rroflow
StateDirectory=rroflow
Restart=on-failure
RestartSec=2
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectKernelLogs=yes
ProtectControlGroups=yes
ProtectClock=yes
ProtectHostname=yes
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
RestrictNamespaces=yes
RestrictRealtime=yes
RestrictSUIDSGID=yes
LockPersonality=yes
MemoryDenyWriteExecute=yes
SystemCallArchitectures=native
CapabilityBoundingSet=
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now rroflow-ingest
systemctl restart rroflow-ingest
sleep 2
systemctl is-active rroflow-ingest

echo "== self-test (selftest devspace; keys not printed)"
base="http://$LISTEN"
curl -sSf "$base/healthz"; echo
curl -sSf -o /dev/null -w 'public %{http_code}\n' "$base/"
curl -sSf -o /dev/null -w 'dashboard %{http_code}\n' "$base/app"
curl -sSf "$base/v1/signals" | head -c 160; echo
st=$(cat /etc/rroflow/keys/selftest.key)
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
curl -sSf -H "Authorization: Bearer $st" -H 'content-type: application/json' "$base/v1/turns" -d @- <<EOF
{"turns":[{"schema":"rroflow.turn/v1","event_id":"selftest-$REV","agent":"selftest","agent_version":"0","provider":"none","model":null,"session_id":"selftest","turn_key":"deploy","subagent":null,"prompt_author":null,"started_at":"$now","ended_at":"$now","cwd":"/","git_branch":"","project":"deploy","usage":{"input_tokens":null,"output_tokens":null,"cache_read_tokens":null,"cache_write_tokens":null,"input_includes_cache":false},"termination":"completed","compacted":false,"tools":[],"tools_unfinished":0,"content":null,"signals":[],"collector":"deploy.sh"}]}
EOF
echo
curl -sSf -H "Authorization: Bearer $st" "$base/v1/signals/settings"; echo
curl -sSf -H "Authorization: Bearer $st" "$base/v1/whoami"; echo
curl -sSf -H "Authorization: Bearer $st" "$base/v1/summary" | grep -o '"turns":[0-9]*' | head -1
runuser -u postgres -- psql -qAt -d rroflow -c "SELECT devspace_id, count(*) FROM turns GROUP BY 1"
curl -s -o /dev/null -w 'wrong key -> %{http_code}\n' -H 'Authorization: Bearer nope' "$base/v1/summary"
ss -ltnp | grep -E ":(${LISTEN##*:}|5432)\b" || true

if [ -n "$PUBLIC_HOSTS" ]; then
  echo "== public TLS (Caddy) for: $PUBLIC_HOSTS"
  # Stop rather than take over ports 80/443 from something else on this host.
  busy=$(ss -ltnpH '( sport = :80 or sport = :443 )' | grep -v '"caddy"' || true)
  if [ -n "$busy" ]; then
    echo "ports 80/443 are used by another service; not installing Caddy:"; echo "$busy"; exit 1
  fi
  apt-get install -y -qq caddy >/dev/null
  sites=$(printf '%s, ' $PUBLIC_HOSTS); sites="${sites%, }"
  cat > /etc/caddy/Caddyfile <<EOF
# rroflow.ai: written by deploy/tx-infra-prod/deploy.sh; changes here are overwritten.
$sites {
	encode zstd gzip
	header {
		Strict-Transport-Security "max-age=31536000"
		-Server
	}
	reverse_proxy $LISTEN
}
EOF
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile >/dev/null
  systemctl enable --now caddy
  systemctl reload caddy
  systemctl is-active caddy
  command -v pve-firewall >/dev/null && echo "proxmox firewall: $(pve-firewall status 2>&1 | head -1)"
  for h in $PUBLIC_HOSTS; do
    ip=$(getent ahostsv4 "$h" | awk 'NR==1{print $1}')
    code=$(curl -s -o /dev/null -m 20 -w '%{http_code}' "https://$h/healthz" || true)
    echo "$h -> ${ip:-no DNS yet}; https /healthz -> ${code:-no answer} (certificate comes once DNS points here)"
  done
fi
echo "== done"
