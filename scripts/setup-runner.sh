#!/usr/bin/env bash
# Run from the repository; requires gh auth login first.
set -euo pipefail
cd "$(dirname "$0")/.."
gh auth status >/dev/null
REPOSITORY=trifazor/apply_tool
STATE="${XDG_DATA_HOME:-$HOME/.local/share}/applytool-release"
mkdir -p "$STATE/runner" "${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
chmod 700 "$STATE/runner"
nix develop --command bash scripts/setup-release.sh
# Keep the NixOS-compatible runner alive through a GC-root symlink.
nix build --inputs-from . nixpkgs#github-runner --out-link "$STATE/runner-package"
export RUNNER_ROOT="$STATE/runner"
if [[ ! -f "$RUNNER_ROOT/.runner" ]]; then
  registration_token=$(gh api --method POST "repos/$REPOSITORY/actions/runners/registration-token" --jq .token)
  "$STATE/runner-package/bin/Runner.Listener" configure --unattended \
    --url "https://github.com/$REPOSITORY" --token "$registration_token" \
    --name applytool-local-x86_64 --labels applytool-builder --work "$STATE/work" --disableupdate
  unset registration_token
fi
UNIT="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/applytool-runner.service"
cat > "$UNIT" <<SERVICE
[Unit]
Description=Apply Tool GitHub release runner
After=network-online.target
[Service]
Environment="RUNNER_ROOT=$STATE/runner"
ExecStart="$STATE/runner-package/bin/Runner.Listener" run
Restart=on-failure
RestartSec=15
[Install]
WantedBy=default.target
SERVICE
systemctl --user daemon-reload
systemctl --user enable --now applytool-runner.service
# The repository must already exist and the authenticated account must administer it.
if gh api "repos/$REPOSITORY/pages" >/dev/null 2>&1; then
  gh api --method PUT "repos/$REPOSITORY/pages" -f build_type=workflow >/dev/null
else
  gh api --method POST "repos/$REPOSITORY/pages" -f build_type=workflow >/dev/null
fi
echo 'Runner started and Pages configured. Push master to build and publish automatically.'
echo 'The runner runs while this user session is active; enable user lingering if it must run after logout.'
