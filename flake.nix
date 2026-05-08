{
  description = "Codex Desktop for Linux installer";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        sourceRoot = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            pkgs.lib.cleanSourceFilter path type
            && (let
              pathStr = toString path;
            in
              !(pkgs.lib.hasSuffix "/.codex" pathStr || pkgs.lib.hasInfix "/.codex/" pathStr));
        };

        codexDmg = pkgs.fetchurl {
          url = "https://persistent.oaistatic.com/codex-app-prod/Codex.dmg";
          hash = "sha256-WSs2iN4Ojk0Ky2FlGsOc8CayZaFHio9Wse+YbpFUE2Y=";
        };

        electronLibs = with pkgs; [
          glib
          gtk3
          pango
          cairo
          gdk-pixbuf
          atk
          at-spi2-atk
          at-spi2-core
          nss
          nspr
          dbus
          cups
          expat
          libdrm
          mesa
          libgbm
          alsa-lib
          libX11
          libXcomposite
          libXdamage
          libXext
          libXfixes
          libXrandr
          libxcb
          libxkbcommon
          libxcursor
          libxi
          libxtst
          libxscrnsaver
          libglvnd
          systemd
          wayland
        ];

        electronLibPath = pkgs.lib.makeLibraryPath electronLibs;
        runtimeLibPath = pkgs.lib.makeLibraryPath (with pkgs; [
          libxcrypt-legacy
          stdenv.cc.cc.lib
          zlib
        ]);
        launcherPath = pkgs.lib.makeBinPath (with pkgs; [
          bash
          coreutils
          curl
          findutils
          gawk
          gnugrep
          gnused
          nodejs
          procps
          python3
          systemd
          xdg-utils
        ]);

        patchNixInstalledApp = installDir: ''
          # Patch generated scripts for NixOS systems without /bin/bash.
          if [ -f "${installDir}/start.sh" ]; then
            ${pkgs.gnused}/bin/sed -i '1s|^#!/bin/bash$|#!${pkgs.bash}/bin/bash|' "${installDir}/start.sh"
            if ! grep -q "NixOS Electron library path" "${installDir}/start.sh"; then
              ${pkgs.gnused}/bin/sed -i '2i# NixOS Electron library path for dlopen()ed GL/EGL libraries.\nexport LD_LIBRARY_PATH="${electronLibPath}:${runtimeLibPath}:''${LD_LIBRARY_PATH:-}"' "${installDir}/start.sh"
            fi
            if ! grep -q "codex_nixos_add_runtime_library_dirs" "${installDir}/start.sh"; then
              ${pkgs.gnused}/bin/sed -i '/^set -euo pipefail$/a\
\
codex_nixos_add_runtime_library_dirs() {\
    local cache_home="''${XDG_CACHE_HOME:-''${HOME:-}/.cache}"\
    local runtime_root="''${CODEX_PRIMARY_RUNTIME_ROOT:-''${CODEX_RUNTIME_ROOT:-$cache_home/codex-runtimes/codex-primary-runtime}}"\
    local dir\
\
    for dir in \\\
        "$runtime_root/dependencies/python/lib" \\\
        "$runtime_root/dependencies/python/lib/python3.12/site-packages/pillow.libs" \\\
        "$runtime_root/dependencies/python/lib/python3.12/site-packages/numpy.libs" \\\
        "$runtime_root/dependencies/node/node_modules/@img/sharp-libvips-linux-x64/lib" \\\
        "$runtime_root/dependencies/node/node_modules/@img/sharp-linux-x64/lib" \\\
        "$runtime_root/dependencies/node/node_modules/@napi-rs/canvas-linux-x64-gnu"; do\
        if [ -d "$dir" ]; then\
            LD_LIBRARY_PATH="$dir:''${LD_LIBRARY_PATH:-}"\
        fi\
    done\
\
    export LD_LIBRARY_PATH\
}\
\
codex_nixos_add_runtime_library_dirs' "${installDir}/start.sh"
            fi
          fi

          # Patch the Electron binary for NixOS.
          if [ -f "${installDir}/electron" ]; then
            echo "[NIX] Patching Electron binary for NixOS..."
            patchelf --set-interpreter "$(cat ${pkgs.stdenv.cc}/nix-support/dynamic-linker)" \
                     --set-rpath "${installDir}:${electronLibPath}" \
                     "${installDir}/electron"

            if [ -f "${installDir}/chrome_crashpad_handler" ]; then
              patchelf --set-interpreter "$(cat ${pkgs.stdenv.cc}/nix-support/dynamic-linker)" \
                       "${installDir}/chrome_crashpad_handler" || true
            fi

            if [ -f "${installDir}/chrome-sandbox" ]; then
              patchelf --set-interpreter "$(cat ${pkgs.stdenv.cc}/nix-support/dynamic-linker)" \
                       "${installDir}/chrome-sandbox" || true
            fi

            find "${installDir}" -maxdepth 1 -name "*.so*" -type f | while read -r so; do
              patchelf --set-rpath "${electronLibPath}" "$so" 2>/dev/null || true
            done

            echo "[NIX] Electron patched successfully"
          fi
        '';

        patchNixGeneratedScripts = installDir: ''
          # Patch generated scripts for NixOS systems without /bin/bash.
          if [ -f "${installDir}/start.sh" ]; then
            ${pkgs.gnused}/bin/sed -i '1s|^#!/bin/bash$|#!${pkgs.bash}/bin/bash|' "${installDir}/start.sh"
          fi
        '';

        codexDesktopPayload = pkgs.stdenv.mkDerivation {
          pname = "codex-desktop-payload";
          version = "unstable-2026-05-02";
          src = sourceRoot;
          __structuredAttrs = true;

          nativeBuildInputs = [
            pkgs.bash
            pkgs.cargo
            pkgs.curl
            pkgs.gcc
            pkgs.gnumake
            pkgs.gnused
            pkgs.makeWrapper
            pkgs.nodejs
            pkgs.p7zip
            pkgs.patchelf
            pkgs.python3
            pkgs.unzip
          ];

          outputHashAlgo = "sha256";
          outputHashMode = "recursive";
          outputHash = "sha256-5bB5LHtOL0x3XAaUrRKRTTxsovHP6VVsgv/dcSfriGs=";
          unsafeDiscardReferences.out = true;

          dontConfigure = true;
          dontBuild = true;

          installPhase = ''
            runHook preInstall

            export HOME="$TMPDIR/home"
            export npm_config_cache="$TMPDIR/npm-cache"
            export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
            export NIX_SSL_CERT_FILE="$SSL_CERT_FILE"
            export npm_config_cafile="$SSL_CERT_FILE"
            export CARGO_HOME="$TMPDIR/cargo-home"
            mkdir -p "$HOME" "$npm_config_cache" "$CARGO_HOME"

            source_dir="$TMPDIR/codex-source"
            mkdir -p "$source_dir"
            cp -R ./. "$source_dir/"
            chmod -R u+w "$source_dir"
            cp ${codexDmg} "$source_dir/Codex.dmg"

            npm_tools="$TMPDIR/npm-tools"
            npm install --prefix "$npm_tools" --ignore-scripts asar @electron/rebuild
            patchShebangs "$npm_tools"
            export PATH="$npm_tools/node_modules/.bin:$PATH"
            substituteInPlace "$source_dir/scripts/lib/asar-patch.sh" \
              --replace-fail "npx --yes asar" "asar" \
              --replace-fail "npx asar" "asar"
            substituteInPlace "$source_dir/scripts/lib/dmg.sh" \
              --replace-fail "npx --yes asar" "asar"
            substituteInPlace "$source_dir/scripts/lib/native-modules.sh" \
              --replace-fail "npx --yes @electron/rebuild" "electron-rebuild"

            export CODEX_INSTALL_DIR="$out/opt/codex-desktop"
            ${pkgs.bash}/bin/bash "$source_dir/install.sh" "$source_dir/Codex.dmg"

            rm -rf "$CODEX_INSTALL_DIR/resources/plugins/openai-bundled/plugins/computer-use"
            marketplace="$CODEX_INSTALL_DIR/resources/plugins/openai-bundled/.agents/plugins/marketplace.json"
            if [ -f "$marketplace" ]; then
              node - "$marketplace" <<'NODE'
              const fs = require("fs");
              const marketplacePath = process.argv[2];
              const marketplace = JSON.parse(fs.readFileSync(marketplacePath, "utf8"));
              marketplace.plugins = (marketplace.plugins || []).filter((plugin) => plugin.name !== "computer-use");
              fs.writeFileSync(marketplacePath, JSON.stringify(marketplace, null, 2) + "\n");
NODE
            fi

            asar extract "$CODEX_INSTALL_DIR/resources/app.asar" "$CODEX_INSTALL_DIR/resources/app-extracted"
            rm -f "$CODEX_INSTALL_DIR/resources/app.asar"
            rm -rf "$CODEX_INSTALL_DIR/resources/app.asar.unpacked"

            ${patchNixGeneratedScripts "$out/opt/codex-desktop"}

            runHook postInstall
          '';
        };

        codexDesktop = pkgs.stdenv.mkDerivation {
          pname = "codex-desktop";
          version = "unstable-2026-05-02";
          src = codexDesktopPayload;

          nativeBuildInputs = [
            pkgs.asar
            pkgs.makeWrapper
            pkgs.patchelf
          ];

          dontConfigure = true;
          dontBuild = true;

          installPhase = ''
            runHook preInstall

            mkdir -p "$out/opt"
            cp -aT "$src/opt/codex-desktop" "$out/opt/codex-desktop"
            chmod -R u+w "$out/opt/codex-desktop"

            resources_dir="$out/opt/codex-desktop/resources"
            (cd "$resources_dir/app-extracted" && find . -type f | LC_ALL=C sort | sed 's#^\./##') > "$TMPDIR/app.asar.ordering"
            asar pack "$resources_dir/app-extracted" "$resources_dir/app.asar" \
              --ordering "$TMPDIR/app.asar.ordering" \
              --unpack "{*.node,*.so,*.dylib}"
            rm -rf "$resources_dir/app-extracted"

            ${patchNixInstalledApp "$out/opt/codex-desktop"}

            install -Dm0644 "$out/opt/codex-desktop/.codex-linux/codex-desktop.png" \
              "$out/share/icons/hicolor/256x256/apps/codex-desktop.png"

            install -Dm0644 ${sourceRoot}/packaging/linux/codex-desktop.desktop \
              "$out/share/applications/codex-desktop.desktop"
            substituteInPlace "$out/share/applications/codex-desktop.desktop" \
              --replace-fail "/usr/bin/codex-desktop" "$out/bin/codex-desktop" \
              --replace-fail "/usr/share/applications/codex-desktop.desktop" "$out/share/applications/codex-desktop.desktop"

            makeWrapper "$out/opt/codex-desktop/start.sh" "$out/bin/codex-desktop" \
              --prefix PATH : "${launcherPath}" \
              --prefix LD_LIBRARY_PATH : "${electronLibPath}" \
              --prefix LD_LIBRARY_PATH : "${runtimeLibPath}" \
              --prefix PATH : "/run/current-system/sw/bin" \
              --prefix PATH : "/etc/profiles/per-user/$(whoami)/bin"

            runHook postInstall
          '';

          meta = {
            description = "Codex Desktop for Linux";
            homepage = "https://github.com/ilysenko/codex-desktop-linux";
            license = pkgs.lib.licenses.mit;
            platforms = pkgs.lib.platforms.linux;
            mainProgram = "codex-desktop";
          };
        };

        installer = pkgs.writeShellApplication {
          name = "codex-desktop-installer";
          runtimeInputs = [
            pkgs.bash
            pkgs.nodejs
            pkgs.python3
            pkgs.p7zip
            pkgs.curl
            pkgs.unzip
            pkgs.gnumake
            pkgs.gcc
            pkgs.patchelf
          ];
          text = ''
            set -euo pipefail

            root_dir="$(pwd)"
            workdir="$(mktemp -d)"
            source_dir="$workdir/source"
            cleanup() {
              rm -rf "$workdir"
            }
            trap cleanup EXIT

            mkdir -p "$source_dir"
            cp -R ${sourceRoot}/. "$source_dir"
            chmod -R u+w "$source_dir"
            cp ${codexDmg} "$source_dir/Codex.dmg"
            chmod +x "$source_dir/install.sh"

            cd "$source_dir"
            export CODEX_INSTALL_DIR="''${CODEX_INSTALL_DIR:-$root_dir/codex-app}"
            ${pkgs.bash}/bin/bash "$source_dir/install.sh" "$source_dir/Codex.dmg" "$@"

            install_dir="''${CODEX_INSTALL_DIR:-$root_dir/codex-app}"

            ${patchNixInstalledApp "$install_dir"}
          '';
        };
      in
      {
        packages = {
          default = codexDesktop;
          codex-desktop = codexDesktop;
          installer = installer;
        };

        apps.default = {
          type = "app";
          program = "${codexDesktop}/bin/codex-desktop";
        };

        apps.installer = {
          type = "app";
          program = "${installer}/bin/codex-desktop-installer";
        };

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.nodejs
            pkgs.python3
            pkgs.p7zip
            pkgs.curl
            pkgs.unzip
            pkgs.gnumake
            pkgs.gcc
          ];
        };
      }
    );
}
