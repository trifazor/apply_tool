#!/usr/bin/env bash
# Builds the frontend and the flatpak, then replaces the installed app with the new build (user install).
set -euo pipefail
cd "$(dirname "$0")"
APP_ID=io.github.trifazor.ApplyTool
nix develop -c pnpm install --frozen-lockfile
nix develop -c pnpm build
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
# build into a local repo first, so the installed app is only touched once the build succeeded
env -u LD_LIBRARY_PATH nix shell nixpkgs#flatpak-builder -c flatpak-builder --user --force-clean \
  --install-deps-from=flathub --repo=repo flatpak-build flatpak/$APP_ID.yml
flatpak kill "$APP_ID" 2>/dev/null || true
flatpak uninstall --user -y "$APP_ID" 2>/dev/null || true
flatpak remote-add --user --no-gpg-verify --if-not-exists applytool-local repo
flatpak install --user -y --reinstall applytool-local "$APP_ID"
echo "Installed. Run with: flatpak run $APP_ID"
