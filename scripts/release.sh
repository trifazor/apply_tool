#!/usr/bin/env bash
# Run inside nix develop. Signing configuration stays outside the checkout.
set -euo pipefail
cd "$(dirname "$0")/.."
CONFIG="${APPLYTOOL_RELEASE_CONFIG:-${XDG_CONFIG_HOME:-$HOME/.config}/applytool-release/config}"
[[ -f "$CONFIG" ]] || { echo "Run scripts/setup-release.sh first." >&2; exit 1; }
source "$CONFIG"
: "${GPG_KEY_ID:?}" "${GNUPGHOME:?}" "${RELEASE_STATE:?}"
export GNUPGHOME
APP_ID=io.github.trifazor.ApplyTool
PAGES_URL=https://trifazor.github.io/apply_tool
RUNTIME_URL=https://dl.flathub.org/repo/flathub.flatpakrepo
[[ $(uname -m) == x86_64 ]] || { echo 'Only Linux x86_64 is supported.' >&2; exit 1; }
mkdir -p "$RELEASE_STATE" .release/site
exec 9>"$RELEASE_STATE/build.lock"
flock 9
# Inspect the exact committed snapshot; don't scan ignored host/build data.
gitleaks git --no-banner --redact --log-opts=-1
BUILD_HASH=$({ printf '%s\n' "$GPG_KEY_ID"; git ls-files -z -- src src-tauri static flatpak package.json pnpm-lock.yaml vite.config.js flake.nix flake.lock | xargs -0 sha256sum; } | sha256sum | cut -d' ' -f1)
if [[ -f "$RELEASE_STATE/ready/hash" && $(cat "$RELEASE_STATE/ready/hash") == "$BUILD_HASH" ]]; then
  rm -rf .release/site
  cp -a "$RELEASE_STATE/ready/site" .release/site
  cp "$RELEASE_STATE/ready/ApplyTool-x86_64.flatpak" "$RELEASE_STATE/ready/SHA256SUMS" .release/
  cp docs/download.html .release/site/index.html
  echo 'Reusing signed artifacts: application build inputs and signing key are unchanged.'
  exit 0
fi
pnpm install --frozen-lockfile
pnpm build
# Nix's desktop-library override conflicts with the system Flatpak executable.
unset LD_LIBRARY_PATH
flatpak remote-add --user --if-not-exists flathub "$RUNTIME_URL"
env -u LD_LIBRARY_PATH flatpak-builder --user --force-clean --arch=x86_64 \
  --default-branch=stable --install-deps-from=flathub \
  --state-dir="$RELEASE_STATE/builder" --repo="$RELEASE_STATE/repo" \
  --gpg-sign="$GPG_KEY_ID" --gpg-homedir="$GNUPGHOME" \
  .release/build flatpak/$APP_ID.yml
flatpak build-update-repo --gpg-sign="$GPG_KEY_ID" --gpg-homedir="$GNUPGHOME" \
  --generate-static-deltas --prune --prune-depth=2 "$RELEASE_STATE/repo"
gpg --batch --export "$GPG_KEY_ID" > .release/public-key.gpg
PUBLIC_KEY=$(base64 -w0 .release/public-key.gpg)
cat > .release/site/apply-tool.flatpakrepo <<REPO
[Flatpak Repo]
Title=Apply Tool
Url=$PAGES_URL/repo/
Homepage=https://github.com/trifazor/apply_tool
GPGKey=$PUBLIC_KEY
REPO
cat > .release/site/apply-tool.flatpakref <<REF
[Flatpak Ref]
Title=Apply Tool
Name=$APP_ID
Branch=stable
Url=$PAGES_URL/repo/
SuggestRemoteName=apply-tool
IsRuntime=false
RuntimeRepo=$RUNTIME_URL
GPGKey=$PUBLIC_KEY
REF
flatpak build-bundle --arch=x86_64 --repo-url="$PAGES_URL/repo/" \
  --runtime-repo="$RUNTIME_URL" --gpg-keys=.release/public-key.gpg \
  "$RELEASE_STATE/repo" .release/ApplyTool-x86_64.flatpak "$APP_ID" stable
# Pages receives a snapshot; the persistent repo retains update history locally.
rm -rf .release/site/repo
cp -a "$RELEASE_STATE/repo" .release/site/repo
cp docs/download.html .release/site/index.html
touch .release/site/.nojekyll
(cd .release; sha256sum ApplyTool-x86_64.flatpak; cd site; sha256sum apply-tool.flatpakref apply-tool.flatpakrepo) > .release/SHA256SUMS
rm -rf "$RELEASE_STATE/ready"
mkdir -p "$RELEASE_STATE/ready"
cp -a .release/site .release/ApplyTool-x86_64.flatpak .release/SHA256SUMS "$RELEASE_STATE/ready/"
printf '%s\n' "$BUILD_HASH" > "$RELEASE_STATE/ready/hash"
echo 'Release artifacts ready in .release; installed app and user data were not modified.'
