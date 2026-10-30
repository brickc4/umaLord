{
  description = "umaLord, an Uma Musume (Global) companion: Linux (Wayland) package and dev shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs, ... }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      # winit and glutin dlopen these at run time, so they go on LD_LIBRARY_PATH, not the linker path.
      runtimeLibs = pkgs: with pkgs; [ wayland libxkbcommon libGL ];
      # What the app runs: curl for live data, art and the Claude API; xdg-open for links and
      # folders; fc-match and fc-list for the UI and Japanese fonts.
      runtimeTools = pkgs: with pkgs; [ curl xdg-utils fontconfig ];
    in
    {
      packages = forAll (pkgs:
        let
          inherit (pkgs) lib;
          version = (lib.importTOML ./app/Cargo.toml).package.version;
          # The app keeps its data folder writable (the account, plans, card art it downloads,
          # the digest), so it runs from a copy of the repo's data in the user's data dir. The
          # copy is refreshed when the package changes; account files are never touched.
          shared = [ "knowledge" "app/assets" "app/templates" "tools/update_knowledge.py" "AGENTS.md" "CLAUDE.md" "opencode.json" "sources.md" ];
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "umalord";
            inherit version;
            src = lib.cleanSource ./.;
            cargoRoot = "app";
            buildAndTestSubdir = "app";
            cargoLock.lockFile = ./app/Cargo.lock;
            # CI runs the tests on every platform; here they'd only slow down the install.
            doCheck = false;
            nativeBuildInputs = with pkgs; [ makeWrapper copyDesktopItems imagemagick ];
            desktopItems = [
              (pkgs.makeDesktopItem {
                name = "umalord";
                desktopName = "umaLord";
                comment = "Plan Uma Musume careers, race builds in a simulator, keep your collection";
                exec = "umalord";
                icon = "umalord";
                startupWMClass = "umalord";
                categories = [ "Game" "Utility" ];
              })
            ];
            postInstall = ''
              data=$out/share/umalord
              mkdir -p $data
              for p in ${lib.escapeShellArgs shared}; do
                mkdir -p "$data/$(dirname "$p")"
                cp -r "$p" "$data/$p"
              done
              mkdir -p $out/share/icons/hicolor/256x256/apps
              magick app/assets/umas/100101.webp $out/share/icons/hicolor/256x256/apps/umalord.png
              wrapProgram $out/bin/umalord \
                --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath (runtimeLibs pkgs)} \
                --prefix PATH : ${lib.makeBinPath (runtimeTools pkgs)} \
                --suffix PATH : ${lib.makeBinPath [ pkgs.opencode ]} \
                --run 'root="''${UMALORD_ROOT:-''${XDG_DATA_HOME:-$HOME/.local/share}/umaLord}"' \
                --run 'if [ "$(cat "$root/.umalord-package" 2>/dev/null)" != "'"$out"'" ]; then mkdir -p "$root" && cp -rT --no-preserve=mode,ownership "'"$data"'" "$root" && echo "'"$out"'" > "$root/.umalord-package"; fi' \
                --run 'export UMALORD_ROOT="$root"'
            '';
            meta = {
              description = "Uma Musume: Pretty Derby (Global) companion: collection, career planner and race simulator";
              homepage = "https://github.com/brickc4/umaLord";
              license = lib.licenses.gpl3Plus;
              mainProgram = "umalord";
              platforms = lib.platforms.linux;
            };
          };
        });

      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            clippy
            rustfmt
            rust-analyzer
            python3 # tools/update_knowledge.py
            opencode # the Ask tab's free backend; the in-app download is Windows-only
          ] ++ runtimeTools pkgs;
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          # A Japanese font for event text, in case the host has none (the app bundles its symbols).
          FONTCONFIG_FILE = pkgs.makeFontsConf { fontDirectories = [ pkgs.noto-fonts-cjk-sans-static ]; };
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (runtimeLibs pkgs)}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
            echo "umaLord: cd app && cargo run --release"
          '';
        };
      });
    };
}
