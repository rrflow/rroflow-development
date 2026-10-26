#!/usr/bin/env bash
# Build and install rroflow-ingest as a hardened systemd service on a Debian host.
# Approved 2026-10-26 for tx-infra-prod (rl07): localhost only, data in /var/lib/rroflow.
# Keys are minted on the host and never printed; only their SHA-256 hashes enter the config.
set -euo pipefail

REV="${REV:?commit to build}"
ROOT=/opt/rroflow
export RUSTUP_HOME=$ROOT/rust/rustup CARGO_HOME=$ROOT/rust/cargo
export PATH=$CARGO_HOME/bin:$PATH
LISTEN="${LISTEN:-127.0.0.1:8787}"

echo "== packages"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq build-essential pkg-config curl ca-certificates >/dev/null

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
[ -s /etc/rroflow/keys/jesse.key ] || mint jesse > /etc/rroflow/keys/jesse.sha256
[ -s /etc/rroflow/keys/selftest.key ] || mint selftest > /etc/rroflow/keys/selftest.sha256
cat > /etc/rroflow/ingest.toml <<EOF
listen = "$LISTEN"
data_dir = "/var/lib/rroflow"
max_batch = 500
max_body_bytes = 16777216

[[devspace]]
id = "jesse"
key_sha256 = ["$(cat /etc/rroflow/keys/jesse.sha256)"]

[[devspace]]
id = "selftest"
key_sha256 = ["$(cat /etc/rroflow/keys/selftest.sha256)"]
EOF
chmod 0644 /etc/rroflow/ingest.toml

echo "== systemd unit"
cat > /etc/systemd/system/rroflow-ingest.service <<'EOF'
[Unit]
Description=rroflow.ai ingest (devspace turns and signals)
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/usr/local/bin/rroflow-ingest serve --config /etc/rroflow/ingest.toml
DynamicUser=yes
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
curl -sSf -o /dev/null -w 'dashboard %{http_code}\n' "$base/"
st=$(cat /etc/rroflow/keys/selftest.key)
now=$(date -u +%Y-%m-%dT%H:%M:%SZ)
curl -sSf -H "Authorization: Bearer $st" -H 'content-type: application/json' "$base/v1/turns" -d @- <<EOF
{"turns":[{"schema":"rroflow.turn/v1","event_id":"selftest-$REV","agent":"selftest","agent_version":"0","provider":"none","model":null,"session_id":"selftest","turn_key":"deploy","subagent":null,"prompt_author":null,"started_at":"$now","ended_at":"$now","cwd":"/","git_branch":"","project":"deploy","usage":{"input_tokens":null,"output_tokens":null,"cache_read_tokens":null,"cache_write_tokens":null,"input_includes_cache":false},"termination":"completed","compacted":false,"tools":[],"tools_unfinished":0,"content":null,"signals":[],"collector":"deploy.sh"}]}
EOF
echo
curl -sSf -H "Authorization: Bearer $st" "$base/v1/summary" | head -c 300; echo
curl -s -o /dev/null -w 'wrong key -> %{http_code}\n' -H 'Authorization: Bearer nope' "$base/v1/summary"
ss -ltnp | grep 8787 || true
echo "== done"
