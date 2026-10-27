#!/usr/bin/env bash
# Run without root on Linux or Git Bash; every platform probe is a local fixture.
set -euo pipefail
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
fixture=$(mktemp -d)
cleanup() {
  rm -f -- "$fixture/bin/uname" "$fixture/bin/hostname" "$fixture/bin/systemd-detect-virt" \
    "$fixture/bin/apt-get" "$fixture/bin/ss" "$fixture/bin/pveversion" "$fixture/output"
  rmdir -- "$fixture/bin" "$fixture"
}
trap cleanup EXIT
mkdir -p "$fixture/bin"
for program in uname hostname systemd-detect-virt apt-get ss; do
  cat > "$fixture/bin/$program" <<'MOCK'
#!/usr/bin/env bash
case "${0##*/}" in
  uname) echo Linux ;;
  hostname) echo fixture-vm ;;
  systemd-detect-virt) [[ "${MOCK_VIRT:-vm}" == vm ]] && echo kvm ;;
  apt-get) exit 0 ;;
  ss) [[ "${MOCK_LISTENER:-}" != occupied ]] || echo 'LISTEN 0 128 *:443 *:* users:(("nginx",pid=1,fd=1))' ;;
esac
MOCK
  chmod +x "$fixture/bin/$program"
done
export PATH="$fixture/bin:$PATH"
export REV=0123456789abcdef0123456789abcdef01234567
export EXPECTED_HOST=fixture-vm
export LISTEN=127.0.0.1:8787
export PUBLIC_HOSTS=
# Test on an ordinary CI runner, never on a Proxmox control host.
if command -v pveversion >/dev/null 2>&1 || [[ -d /etc/pve ]]; then
  echo 'Tests require an ordinary development/CI host.' >&2; exit 1
fi
pass() { bash "$script_dir/preflight.sh" > "$fixture/output" 2>&1 || { cat "$fixture/output"; exit 1; }; }
reject() {
  local expected=$1
  shift
  if env "$@" bash "$script_dir/preflight.sh" > "$fixture/output" 2>&1; then
    echo "Expected refusal: $expected" >&2; exit 1
  fi
  grep -qF "$expected" "$fixture/output" || { cat "$fixture/output"; exit 1; }
}
pass
reject 'Set EXPECTED_HOST' EXPECTED_HOST=
reject 'Hostname does not match' EXPECTED_HOST=another-vm
reject 'verify that this is a VM' MOCK_VIRT=bare-metal
reject 'full lowercase Git commit SHA' REV=main
reject 'IPv4 loopback' LISTEN=0.0.0.0:8787
reject 'outside 1..65535' LISTEN=127.0.0.1:99999
reject 'only DNS hostnames' PUBLIC_HOSTS='bad;host'
reject 'occupied by another' PUBLIC_HOSTS=example.invalid MOCK_LISTENER=occupied
printf '#!/usr/bin/env bash\nexit 0\n' > "$fixture/bin/pveversion"
chmod +x "$fixture/bin/pveversion"
reject 'Proxmox host'
echo 'PASS: 10 deployment guard cases'
