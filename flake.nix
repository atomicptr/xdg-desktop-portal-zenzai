{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  description = "A collection of xdg-desktop-portal implementations for lightweight wayland compositors";

  outputs =
    { nixpkgs, ... }:
    let
      inherit (nixpkgs) lib;

      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      package = pkgs: pkgs.callPackage ./nix/package.nix { };
    in
    {
      packages = forAllSystems (pkgs: {
        default = package pkgs;
        xdg-desktop-portal-zenzai = package pkgs;
      });

      overlays.default = final: _: { xdg-desktop-portal-zenzai = package final; };

      nixosModules.default = import ./nix/nixos-module.nix { defaultPackage = package; };

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          nativeBuildInputs = [
            pkgs.pkg-config
            pkgs.rustPlatform.bindgenHook
          ];

          buildInputs = [ pkgs.cups ];

          packages = with pkgs; [
            cargo
            clippy
            just
            rust-analyzer
            rustc
            rustfmt
          ];

          LD_LIBRARY_PATH = lib.makeLibraryPath [
            pkgs.libxkbcommon
            pkgs.wayland
          ];
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);

      checks = forAllSystems (pkgs: {
        package = package pkgs;

        module-config =
          let
            evaluate =
              config:
              lib.evalModules {
                modules = [
                  {
                    options.services.xdg-desktop-portal-zenzai = import ./nix/options.nix {
                      inherit lib;
                      defaultPackage = package pkgs;
                    };
                    config.services.xdg-desktop-portal-zenzai = config;
                  }
                ];
              };
            module = evaluate (import ./nix/tests/full-config.nix);
            defaults = (evaluate { }).config.services.xdg-desktop-portal-zenzai;
            optedOut = (evaluate { portals.usb.enable = false; }).config.services.xdg-desktop-portal-zenzai;
            disabledConfig = import ./nix/config.nix { inherit lib; } optedOut;

            generated = import ./nix/config.nix {
              inherit lib;
            } module.config.services.xdg-desktop-portal-zenzai;
          in
          assert lib.all (portal: portal.enable) (builtins.attrValues defaults.portals);
          assert !disabledConfig.usb.enabled;
          assert lib.all (portal: portal.enable) (
            builtins.attrValues (builtins.removeAttrs optedOut.portals [ "usb" ])
          );
          assert generated == lib.importTOML ./crates/xdg-desktop-portal-zenzai/tests/full-config.toml;
          pkgs.emptyFile;
      });
    };
}
