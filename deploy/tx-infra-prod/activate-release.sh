#!/usr/bin/env bash
# Activate an already-reviewed CI artifact on an already-provisioned application VM.
# No package installation, configuration rewrite, credentials minting or proxy changes.
set +x
set -euo pipefail
umask 077
die() { printf 'REFUSED: %s\n' "$*" >&2; exit 1; }
[[ $# == 3 ]] || die 'Usage: activate-release.sh ARCHIVE EXPECTED_SHA256 EXPECTED_COMMIT'
archive=$(realpath -e -- "$1")
expected_digest=$2
export REV=$3
[[ "$expected_digest" =~ ^[0-9a-f]{64}$ ]] || die 'Expected SHA-256 must come from the verified GitHub artifact.'
[[ "$REV" =~ ^[0-9a-f]{40}$ ]] || die 'Expected revision must be a full commit SHA.'
[[ $EUID == 0 ]] || die 'Activation requires the deployment operator.'
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
export LISTEN=${LISTEN:-127.0.0.1:8787}
export PUBLIC_HOSTS=
bash "$script_dir/preflight.sh"
[[ "$(uname -m)" == x86_64 ]] || die 'This artifact supports x86_64 only.'
for command in sha256sum tar curl flock systemctl stat sync; do command -v "$command" >/dev/null || die "Missing deployment prerequisite: $command"; done
[[ "$(sha256sum "$archive" | cut -d' ' -f1)" == "$expected_digest" ]] || die 'Artifact checksum mismatch.'

root=${RROFLOW_RELEASE_ROOT:-/opt/rroflow}
config=${RROFLOW_CONFIG:-/etc/rroflow/ingest.toml}
service=rroflow-ingest.service
[[ "$root" == /* && "$root" != / && ! -L "$root" ]] || die 'Release root must be an explicit nonsymlink directory.'
root=$(realpath -e -- "$root")
[[ -d "$root/releases" && ! -L "$root/releases" ]] || die 'Provision the releases directory first.'
secure_file() {
  [[ -f "$1" && ! -L "$1" && "$(stat -c %u "$1")" == 0 ]] || die 'Deployment input must be a root-owned regular file.'
  local mode; mode=$(stat -c %a "$1"); (( (8#$mode & 8#022) == 0 )) || die 'Deployment input is group/world writable.'
}
secure_release() {
  local dir=$1 mode file
  [[ -d "$dir" && ! -L "$dir" && "$(stat -c %u "$dir")" == 0 ]] || die 'Managed release must be a root-owned directory.'
  mode=$(stat -c %a "$dir"); (( (8#$mode & 8#022) == 0 )) || die 'Managed release is group/world writable.'
  for file in revision target migrations build-info.json SHA256SUMS rroflow-ingest; do secure_file "$dir/$file"; done
}
for dir in "$root" "$root/releases"; do
  [[ "$(stat -c %u "$dir")" == 0 ]] || die 'Release directories must be root-owned.'
  mode=$(stat -c %a "$dir"); (( (8#$mode & 8#022) == 0 )) || die 'Release directories are group/world writable.'
done
secure_file "$config"
secure_file "${RROFLOW_BACKUP_RECEIPT:?Set RROFLOW_BACKUP_RECEIPT to the reviewed backup/restore evidence receipt}"
[[ -s "$RROFLOW_BACKUP_RECEIPT" ]] || die 'Backup receipt is empty.'
secure_file "${RROFLOW_SMOKE_KEY_FILE:?Set RROFLOW_SMOKE_KEY_FILE to a protected ingest key for authenticated smoke checks}"
key_mode=$(stat -c %a "$RROFLOW_SMOKE_KEY_FILE"); (( (8#$key_mode & 8#077) == 0 )) || die 'Smoke key must be private to the operator.'
[[ "${RROFLOW_SMOKE_DEVSPACE:-}" =~ ^[A-Za-z0-9_-]{1,64}$ ]] || die 'Set the expected smoke devspace.'
unit=$(systemctl show "$service" --property=ExecStart --value)
[[ "$unit" == *"path=$root/current/rroflow-ingest ;"* && "$unit" == *"--config $config"* ]] || die 'Service does not use the managed current release and expected config.'
[[ "$(systemctl show "$service" --property=User --value)" == rroflow ]] || die 'Service must run as rroflow.'

exec 9>"$root/deploy.lock"
flock -n 9 || die 'Another activation holds the deployment lock.'
[[ ! -e "$root/activation.pending" ]] || die 'An interrupted activation needs operator recovery; inspect activation.pending.'
[[ ! -e "$root/current" || -L "$root/current" ]] || die 'current must be a managed symlink.'
previous=''
if [[ -L "$root/current" ]]; then
  previous=$(readlink "$root/current")
  [[ "$previous" =~ ^releases/[0-9a-f]{40}$ && -d "$root/$previous" && ! -L "$root/$previous" ]] || die 'Existing current target is not a managed release.'
  secure_release "$root/$previous"
fi
stage=$(mktemp -d "$root/releases/.stage.XXXXXX")
changed=0
swap() { ln -s "$1" "$root/.current.$$" && mv -Tf -- "$root/.current.$$" "$root/current" && sync -f "$root"; }
smoke() {
  local revision=$1 fingerprint=$2 info who key
  systemctl is-active --quiet "$service" || return 1
  curl --noproxy '*' --fail --silent --max-time 3 "http://$LISTEN/readyz" >/dev/null || return 1
  info=$(curl --noproxy '*' --fail --silent --max-time 3 "http://$LISTEN/build-info") || return 1
  [[ "$info" == *"\"revision\":\"$revision\""* && "$info" == *"\"migrations\":\"$fingerprint\""* ]] || return 1
  key=$(cat "$RROFLOW_SMOKE_KEY_FILE")
  [[ -n "$key" && "$key" != *$'\n'* && "$key" != *$'\r'* ]] || return 1
  who=$(printf 'Authorization: Bearer %s\n' "$key" | curl --noproxy '*' --fail --silent --max-time 3 --header @- "http://$LISTEN/v1/whoami") || return 1
  [[ "$who" == *"\"devspace\":\"$RROFLOW_SMOKE_DEVSPACE\""* ]] || return 1
  printf 'Authorization: Bearer %s\n' "$key" | curl --noproxy '*' --fail --silent --max-time 3 --header @- "http://$LISTEN/v1/summary" >/dev/null || return 1
}
wait_smoke() {
  local attempt
  for attempt in {1..10}; do
    if smoke "$1" "$2"; then return 0; fi
    sleep 1
  done
  return 1
}
cleanup() {
  local status=$?
  trap - EXIT
  if (( changed )); then
    printf 'Activation failed; restoring prior release.\n' >&2
    if [[ -n "$previous" ]]; then
      if swap "$previous" && systemctl restart "$service" && wait_smoke "${previous#releases/}" "$(cat "$root/$previous/migrations")"; then
        printf 'active=%s\nfailed_candidate=releases/%s\nverified_at=%s\n' "$previous" "$REV" "$(date -u +%FT%TZ)" > "$root/last-rollback.receipt"
        sync -f "$root/last-rollback.receipt"
        rm -f -- "$root/activation.pending"
        printf 'Prior release restored and authenticated smoke passed.\n' >&2
      else
        printf 'ROLLBACK NEEDS OPERATOR: pending receipt retained.\n' >&2
      fi
    else
      systemctl stop "$service" || true
      [[ "$(readlink "$root/current")" != "releases/$REV" ]] || rm -- "$root/current"
      printf 'First activation failed; service stopped, pending receipt retained.\n' >&2
    fi
  fi
  # stage is made by mktemp inside the verified release root; never delete release data.
  if [[ "$stage" == "$root/releases/.stage."* && -d "$stage" && ! -L "$stage" ]]; then rm -rf -- "$stage"; fi
  rm -f -- "$root/.current.$$"
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
# Snapshot under the root-owned staging directory, then verify again. A mutable
# download directory must not permit replacing the archive between check/extract.
cp -- "$archive" "$stage/.archive"
archive="$stage/.archive"
[[ "$(sha256sum "$archive" | cut -d' ' -f1)" == "$expected_digest" ]] || die 'Artifact changed while staging.'
expected_members=$'SHA256SUMS\nbuild-info.json\nmigrations\nrevision\nrroflow-ingest\ntarget'
[[ "$(tar -tzf "$archive" | LC_ALL=C sort)" == "$expected_members" ]] || die 'Archive has unexpected or duplicate members.'
[[ -z "$(tar -tvzf "$archive" | grep -v '^-' || true)" ]] || die 'Archive members must be regular files.'
tar --extract --gzip --file "$archive" --directory "$stage" --no-same-owner --no-same-permissions
[[ "$(cat "$stage/revision")" == "$REV" && "$(cat "$stage/target")" == x86_64-unknown-linux-gnu ]] || die 'Artifact identity mismatch.'
fingerprint=$(cat "$stage/migrations")
[[ "$fingerprint" =~ ^[0-9a-f]{64}$ ]] || die 'Invalid migration fingerprint.'
[[ "$(wc -l < "$stage/SHA256SUMS")" == 5 ]] || die 'Invalid checksum inventory.'
[[ "$(sed -E 's/^[0-9a-f]{64}  //' "$stage/SHA256SUMS" | LC_ALL=C sort)" == $'build-info.json\nmigrations\nrevision\nrroflow-ingest\ntarget' ]] || die 'Invalid checksum paths.'
(cd "$stage" && sha256sum --check --strict SHA256SUMS >/dev/null) || die 'Release member checksum mismatch.'
chmod 0755 "$stage/rroflow-ingest"
[[ "$("$stage/rroflow-ingest" build-info)" == "$(cat "$stage/build-info.json")" ]] || die 'Embedded binary identity mismatch.'
rm -- "$stage/.archive"
if [[ -n "$previous" ]]; then
  [[ "$(cat "$root/$previous/migrations")" == "$fingerprint" ]] || die 'Migration change requires a separately reviewed migration/restore procedure.'
fi
destination="$root/releases/$REV"
if [[ -e "$destination" ]]; then
  [[ ! -L "$destination" && -d "$destination" ]] || die 'Release target is not a directory.'
  secure_release "$destination"
  (cd "$destination" && sha256sum --check --strict "$stage/SHA256SUMS" >/dev/null) || die 'Existing immutable release differs.'
else
  chmod 0755 "$stage"
  chmod 0644 "$stage/"{revision,target,migrations,build-info.json,SHA256SUMS}
  mv -- "$stage" "$destination"
fi
printf 'previous=%s\ncandidate=releases/%s\narchive_sha256=%s\n' "$previous" "$REV" "$expected_digest" > "$root/activation.pending"
sync -f "$root/activation.pending"
changed=1
swap "releases/$REV"
systemctl restart "$service"
wait_smoke "$REV" "$fingerprint" || die 'Candidate readiness, identity or authenticated smoke did not pass.'
printf 'revision=%s\narchive_sha256=%s\nprevious=%s\nmigrations=%s\nverified_at=%s\n' "$REV" "$expected_digest" "$previous" "$fingerprint" "$(date -u +%FT%TZ)" > "$root/last-deployment.receipt"
sync -f "$root/last-deployment.receipt"
changed=0
rm -- "$root/activation.pending"
sync -f "$root"
printf 'Activated %s; readiness, build identity and authenticated smoke passed.\n' "$REV"
