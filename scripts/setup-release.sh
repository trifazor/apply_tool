#!/usr/bin/env bash
# Create a dedicated unattended signing key, outside Git and the app sandbox data.
set -euo pipefail
umask 077
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/applytool-release"
RELEASE_STATE="${XDG_DATA_HOME:-$HOME/.local/share}/applytool-release"
mkdir -p "$CONFIG_DIR" "$RELEASE_STATE/gnupg"
chmod 700 "$CONFIG_DIR" "$RELEASE_STATE/gnupg"
export GNUPGHOME="$RELEASE_STATE/gnupg"
if [[ -f "$CONFIG_DIR/config" ]]; then
  echo "Release signing configuration already exists at $CONFIG_DIR/config"
  exit 0
fi
gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key \
  'Apply Tool Releases <releases@apply-tool.local>' rsa3072 sign 0
GPG_KEY_ID=$(gpg --batch --with-colons --list-secret-keys | awk -F: '$1=="fpr" {print $10; exit}')
printf 'GPG_KEY_ID=%q\nGNUPGHOME=%q\nRELEASE_STATE=%q\n' \
  "$GPG_KEY_ID" "$GNUPGHOME" "$RELEASE_STATE" > "$CONFIG_DIR/config"
echo "Signing key created outside the repo. Back up $RELEASE_STATE/gnupg securely."
