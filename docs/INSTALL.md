# Install Apply Tool on Linux x86_64

Install Flatpak using your distribution's package manager. Download `apply-tool.flatpakref` from [GitHub Releases](https://github.com/trifazor/apply_tool/releases/latest) and open it with your software manager, or run:

```bash
flatpak install --user https://trifazor.github.io/apply_tool/apply-tool.flatpakref
flatpak run io.github.trifazor.ApplyTool
```

This installs the app from our signed repository and registers its update source. The app is not published on Flathub; its GNOME runtime is downloaded from Flathub. The standalone `.flatpak` download also includes the update repository URL and public signing key.

For updates, use your software manager or:

```bash
flatpak update --user
```

Your desktop's settings determine whether updates install automatically. You don't need to download each release again. Documents and settings are kept in `~/ApplyTool`; app updates preserve them.

Install and authenticate Claude Code, Codex or OpenCode on the host to generate documents with AI. Choose your agent and model in Settings. Typst and Pandoc are bundled.
