{
  description = "A collection of xdg-desktop-portal implementations for lightweight wayland compositors";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

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
          inputsFrom = [ (package pkgs) ];

          packages = with pkgs; [
            clippy
            rust-analyzer
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

        # the module options must produce exactly what the Rust config parser is tested against
        module-config =
          let
            module = lib.evalModules {
              modules = [
                {
                  options.services.xdg-desktop-portal-zenzai = import ./nix/options.nix {
                    inherit lib;
                    defaultPackage = package pkgs;
                  };
                  config.services.xdg-desktop-portal-zenzai = import ./nix/tests/full-config.nix;
                }
              ];
            };

            generated = import ./nix/config.nix {
              inherit lib;
            } module.config.services.xdg-desktop-portal-zenzai;
          in
          assert generated == lib.importTOML ./crates/xdg-desktop-portal-zenzai/tests/full-config.toml;
          pkgs.emptyFile;
      });
    };
}
