#!/usr/bin/env bash
# Linux CI only. All service, host and HTTP calls are fixtures inside a private directory.
set -euo pipefail
[[ $(uname -s) == Linux && $EUID == 0 ]] || { echo 'Run this fixture suite as root on a Linux CI runner.' >&2; exit 1; }
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
fixture=$(mktemp -d /tmp/rroflow-release-test.XXXXXX)
cleanup() { [[ "$fixture" == /tmp/rroflow-release-test.* && ! -L "$fixture" ]] && rm -rf -- "$fixture"; }
trap cleanup EXIT
mkdir "$fixture/bin" "$fixture/root" "$fixture/root/releases" "$fixture/artifacts"
export RROFLOW_RELEASE_ROOT="$fixture/root" RROFLOW_CONFIG="$fixture/config.toml"
export RROFLOW_BACKUP_RECEIPT="$fixture/backup-receipt" RROFLOW_SMOKE_KEY_FILE="$fixture/smoke-key" RROFLOW_SMOKE_DEVSPACE=fixture
export EXPECTED_HOST=fixture-vm LISTEN=127.0.0.1:8787
printf 'fixture only\n' > "$RROFLOW_CONFIG"
printf 'fixture restore rehearsal, not production evidence\n' > "$RROFLOW_BACKUP_RECEIPT"
printf 'fixture-private-key\n' > "$RROFLOW_SMOKE_KEY_FILE"
chmod 0600 "$RROFLOW_CONFIG" "$RROFLOW_BACKUP_RECEIPT" "$RROFLOW_SMOKE_KEY_FILE"
for program in uname hostname systemd-detect-virt apt-get systemctl curl sleep; do
  cat > "$fixture/bin/$program" <<'MOCK'
#!/usr/bin/env bash
set -eu
case "${0##*/}" in
  uname) [[ "${1:-}" == -m ]] && echo x86_64 || echo Linux ;;
  hostname) echo fixture-vm ;;
  systemd-detect-virt) echo kvm ;;
  apt-get|sleep) exit 0 ;;
  systemctl)
    case "$*" in
      *--property=ExecStart*) printf '{ path=%s/current/rroflow-ingest ; argv[]=%s/current/rroflow-ingest serve --config %s ; }\n' "${MOCK_UNIT_ROOT:-$RROFLOW_RELEASE_ROOT}" "$RROFLOW_RELEASE_ROOT" "$RROFLOW_CONFIG" ;;
      *--property=User*) echo rroflow ;;
      restart*) printf 'restart\n' >> "$RROFLOW_RELEASE_ROOT/service-calls" ;;
      *) exit 0 ;;
    esac ;;
  curl)
    revision=$(cat "$RROFLOW_RELEASE_ROOT/current/revision")
    endpoint=${!#}
    if [[ "$*" == *--header* ]]; then
      auth=$(cat)
      [[ "$auth" == 'Authorization: Bearer fixture-private-key' ]] || exit 22
      [[ "${MOCK_AUTH_FAILURE:-}" != "$revision" ]] || exit 22
    fi
    case "$endpoint" in
      */readyz) [[ "${MOCK_READY_FAILURE:-}" != "$revision" ]] || exit 22; echo ready ;;
      */build-info) cat "$RROFLOW_RELEASE_ROOT/current/build-info.json" ;;
      */v1/whoami) printf '{"devspace":"fixture"}\n' ;;
      */v1/summary) printf '{"summary":{"turns":0}}\n' ;;
      *) exit 22 ;;
    esac ;;
esac
MOCK
  chmod +x "$fixture/bin/$program"
done
export PATH="$fixture/bin:$PATH"
old=$(printf '1%.0s' {1..40}); new=$(printf '2%.0s' {1..40}); changed=$(printf '3%.0s' {1..40})
fingerprint=$(printf 'a%.0s' {1..64})
bundle() {
  local revision=$1 migration=$2 source="$fixture/source-$1" archive="$fixture/artifacts/rroflow-ingest-$1-x86_64-unknown-linux-gnu.tar.gz"
  mkdir "$source"
  printf '#!/usr/bin/env bash\nprintf '\''{"revision":"%s","target":"x86_64-unknown-linux-gnu","migrations":"%s"}\\n'\''\n' "$revision" "$migration" > "$source/rroflow-ingest"
  chmod +x "$source/rroflow-ingest"
  "$source/rroflow-ingest" build-info > "$source/build-info.json"
  printf '%s\n' "$revision" > "$source/revision"
  printf '%s\n' "$migration" > "$source/migrations"
  printf 'x86_64-unknown-linux-gnu\n' > "$source/target"
  (cd "$source" && sha256sum build-info.json migrations revision rroflow-ingest target > SHA256SUMS)
  tar -czf "$archive" -C "$source" build-info.json migrations revision rroflow-ingest target SHA256SUMS
  sha256sum "$archive" | cut -d' ' -f1 > "$archive.sha256"
}
bundle "$old" "$fingerprint"; bundle "$new" "$fingerprint"; bundle "$changed" "$(printf 'b%.0s' {1..64})"
archive() { printf '%s/artifacts/rroflow-ingest-%s-x86_64-unknown-linux-gnu.tar.gz' "$fixture" "$1"; }
activate() { local a; a=$(archive "$1"); bash "$script_dir/activate-release.sh" "$a" "$(cat "$a.sha256")" "$1"; }
reject() {
  local message=$1; shift
  if "$@" > "$fixture/output" 2>&1; then cat "$fixture/output"; echo "Expected refusal: $message" >&2; exit 1; fi
  grep -qF "$message" "$fixture/output" || { cat "$fixture/output"; exit 1; }
}
activate "$old" > "$fixture/output"
[[ $(readlink "$RROFLOW_RELEASE_ROOT/current") == "releases/$old" ]]
activate "$new" > "$fixture/output"
grep -qF "revision=$new" "$RROFLOW_RELEASE_ROOT/last-deployment.receipt"
activate "$old" > "$fixture/output"
export MOCK_READY_FAILURE=$new
reject 'Prior release restored and authenticated smoke passed.' activate "$new"
[[ $(readlink "$RROFLOW_RELEASE_ROOT/current") == "releases/$old" && ! -e "$RROFLOW_RELEASE_ROOT/activation.pending" ]]
unset MOCK_READY_FAILURE
export MOCK_AUTH_FAILURE=$new
reject 'Prior release restored and authenticated smoke passed.' activate "$new"
unset MOCK_AUTH_FAILURE
[[ $(readlink "$RROFLOW_RELEASE_ROOT/current") == "releases/$old" ]]
calls=$(wc -l < "$RROFLOW_RELEASE_ROOT/service-calls")
reject 'Artifact checksum mismatch' bash "$script_dir/activate-release.sh" "$(archive "$new")" "$(printf '0%.0s' {1..64})" "$new"
reject 'Migration change requires' activate "$changed"
printf 'interrupted fixture\n' > "$RROFLOW_RELEASE_ROOT/activation.pending"
reject 'interrupted activation needs' activate "$new"
rm "$RROFLOW_RELEASE_ROOT/activation.pending"
reject 'Another activation holds' flock "$RROFLOW_RELEASE_ROOT/deploy.lock" bash "$script_dir/activate-release.sh" "$(archive "$new")" "$(cat "$(archive "$new").sha256")" "$new"
export MOCK_UNIT_ROOT=/unmanaged
reject 'Service does not use' activate "$new"
unset MOCK_UNIT_ROOT
chmod 0644 "$RROFLOW_SMOKE_KEY_FILE"
reject 'Smoke key must be private' activate "$new"
chmod 0600 "$RROFLOW_SMOKE_KEY_FILE"
printf 'unexpected member\n' > "$fixture/source-$new/unexpected"
tar -czf "$fixture/extra.tar.gz" -C "$fixture/source-$new" build-info.json migrations revision rroflow-ingest target SHA256SUMS unexpected
reject 'Archive has unexpected or duplicate members' bash "$script_dir/activate-release.sh" "$fixture/extra.tar.gz" "$(sha256sum "$fixture/extra.tar.gz" | cut -d' ' -f1)" "$new"
printf 'outside sentinel\n' > "$fixture/outside"
rm "$fixture/source-$new/rroflow-ingest"
ln -s "$fixture/outside" "$fixture/source-$new/rroflow-ingest"
tar -czf "$fixture/symlink.tar.gz" -C "$fixture/source-$new" build-info.json migrations revision rroflow-ingest target SHA256SUMS
reject 'Archive members must be regular files' bash "$script_dir/activate-release.sh" "$fixture/symlink.tar.gz" "$(sha256sum "$fixture/symlink.tar.gz" | cut -d' ' -f1)" "$new"
[[ $(cat "$fixture/outside") == 'outside sentinel' ]]
printf 'tamper\n' >> "$RROFLOW_RELEASE_ROOT/releases/$new/rroflow-ingest"
reject 'Existing immutable release differs' activate "$new"
[[ $(wc -l < "$RROFLOW_RELEASE_ROOT/service-calls") == "$calls" ]]
grep -qF "active=releases/$old" "$RROFLOW_RELEASE_ROOT/last-rollback.receipt"
original_root=$RROFLOW_RELEASE_ROOT
export RROFLOW_RELEASE_ROOT="$fixture/first-failure"
mkdir "$RROFLOW_RELEASE_ROOT" "$RROFLOW_RELEASE_ROOT/releases"
export MOCK_READY_FAILURE=$new
reject 'First activation failed; service stopped, pending receipt retained.' activate "$new"
[[ -f "$RROFLOW_RELEASE_ROOT/activation.pending" && ! -L "$RROFLOW_RELEASE_ROOT/current" ]]
export RROFLOW_RELEASE_ROOT=$original_root
unset MOCK_READY_FAILURE
echo 'PASS: first activation/failure, upgrade, explicit rollback, readiness/auth failure rollback, checksum/migration/pending/lock/unit/key/archive/immutable-release refusal.'
