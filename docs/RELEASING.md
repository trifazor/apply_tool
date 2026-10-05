# Releases built on your machine

Every push to `master` in `trifazor/apply_tool` queues the release workflow. Your local Linux x86_64 runner builds and signs the Flatpak, deploys the update repository to GitHub Pages, and publishes the GitHub Release only after deployment succeeds. Both jobs run on your machine. Pull requests do not trigger this runner. Signed artifacts are reused when the application build inputs and signing key have not changed; documentation-only pushes do not require another Rust build.

## One-time setup

```bash
gh auth login
./scripts/setup-runner.sh
git push -u origin master
```

You must administer the GitHub repository to register the runner and enable Pages. The runner needs your Nix installation, network access, Flatpak and the GNOME SDK. Signing material and build state are outside Git:

- `~/.config/applytool-release/config`: signing key fingerprint and paths.
- `~/.local/share/applytool-release/gnupg`: dedicated private signing key (mode 0700).
- `~/.local/share/applytool-release/repo`: persistent signed OSTree repository.
- `~/.local/share/applytool-release/builder`: build cache.
- `~/.local/share/applytool-release/runner`: GitHub runner registration credentials.

The signing key is generated without a passphrase for unattended releases. Back it up securely; changing it later requires migrating users' trust. Never upload that directory, runner credentials or your real CV. Review code before pushing: a release workflow on this machine has access to the build user's files and signing key. Use a dedicated build user/VM for a public project when practical.

## Local verification and manual build

```bash
nix develop --command bash scripts/release.sh
systemctl --user status applytool-runner.service
journalctl --user -u applytool-runner.service
```

The release build does not uninstall your desktop app. Generated installers are in `.release/`; they are ignored by Git. The updater uses the fixed app ID and `stable` Flatpak branch. Each release has a unique `build-<commit>` tag; normal pushes do not require a manual tag. Update the versions in `package.json`, `src-tauri/Cargo.toml`, Cargo.lock and `src-tauri/tauri.conf.json` when making a named product version.

Keep the computer on and connected when pushing. The user service starts at login. If it must continue after logout, configure user lingering with your system administrator.

The Nix runner has self-updates disabled because the Nix store is immutable. Refresh it regularly by rebuilding `runner-package` with a current nixpkgs revision and restarting the service; GitHub can stop accepting jobs from an outdated runner. Keep the same RUNNER_ROOT so its registration survives.

Publishing failures keep the GitHub Release in draft. Retry the workflow from GitHub Actions; existing assets are replaced. Preserve the persistent OSTree repository for efficient updates. The pipeline serializes builds and keeps two parents when pruning.
