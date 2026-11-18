#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [[ "$(uname -s)" != Linux ]]; then
  echo "FG-C3.7 BLOCKED: Linux cgroup v2 is required." >&2
  exit 2
fi
if [[ "$(stat -fc %T /sys/fs/cgroup 2>/dev/null || true)" != cgroup2fs ]]; then
  echo "FG-C3.7 BLOCKED: /sys/fs/cgroup is not a cgroup v2 mount." >&2
  exit 2
fi
if ! command -v systemd-run >/dev/null || ! systemctl --user is-system-running >/dev/null 2>&1; then
  echo "FG-C3.7 BLOCKED: an active user systemd manager is required; uncontained execution is not substituted." >&2
  exit 2
fi
controllers="$(cat /sys/fs/cgroup/cgroup.controllers)"
for controller in cpu memory pids; do
  if ! grep -qw "$controller" <<<"$controllers"; then
    echo "FG-C3.7 BLOCKED: cgroup v2 $controller controller is unavailable." >&2
    exit 2
  fi
done
if ! command -v python3 >/dev/null; then
  echo "FG-C3.7 BLOCKED: python3 is required for the bounded OOM workload." >&2
  exit 2
fi

cargo build --manifest-path spikes/fg-c3-2-1/rust-planner/Cargo.toml --locked
test_binary="$repo_root/.fg-c3-7-test-$$"
trap 'rm -f "$test_binary"' EXIT
go -C spikes/fg-c3-2-1/go-coordinator test -c -o "$test_binary"

unit="logos-fg-c3-7-$(date +%s)-$$"
echo "FG-C3.7: launching disposable unit $unit with bounded cgroup and process limits."
systemd-run --user --wait --collect --pipe --expand-environment=no \
  --unit="$unit" \
  --working-directory="$repo_root/spikes/fg-c3-2-1/go-coordinator" \
  --setenv=FG_C3_7_RUN=1 \
  --setenv="FG_C3_2_2_RUST_PLANNER=$repo_root/spikes/fg-c3-2-1/rust-planner/target/debug/fg-c3-2-1-rust-planner-spike" \
  --property=MemoryMax=384M \
  --property=MemorySwapMax=0 \
  --property=CPUQuota=50% \
  --property=TasksMax=64 \
  --property=LimitNOFILE=128 \
  --property=OOMPolicy=continue \
  --property=KillMode=control-group \
  --property=TimeoutStopSec=3s \
  --property=NoNewPrivileges=yes \
  --property=ProtectSystem=strict \
  --property=ProtectHome=read-only \
  --property=PrivateTmp=yes \
  --property=PrivateNetwork=yes \
  --property=ProtectControlGroups=yes \
  -- "$test_binary" -test.run '^TestFGC37SystemdWorkload$' -test.count=1
