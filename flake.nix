{
  description = "apply-tool dev shell (Tauri 2 + SvelteKit)";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      libs = with pkgs; [ webkitgtk_4_1 gtk3 libsoup_3 glib cairo pango gdk-pixbuf atk openssl librsvg dbus ];
    in {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [ pkg-config rustc cargo rustfmt clippy nodejs_22 pnpm flatpak-builder wrapGAppsHook3 typst pandoc git gh gnupg gitleaks ];
        buildInputs = libs;
        shellHook = ''
          export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libs}:$LD_LIBRARY_PATH
          export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
          export WEBKIT_DISABLE_DMABUF_RENDERER=1
        '';
      };
    };
}
