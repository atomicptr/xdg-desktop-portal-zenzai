{ defaultPackage }:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.xdg-desktop-portal-zenzai;
in
{
  options.services.xdg-desktop-portal-zenzai = import ./options.nix {
    inherit lib;
    defaultPackage = defaultPackage pkgs;
  };

  config = lib.mkIf cfg.enable {
    # User config takes precedence.
    environment.etc."xdg/xdg-desktop-portal-zenzai/config.toml".source =
      (pkgs.formats.toml { }).generate "config.toml"
        (import ./config.nix { inherit lib; } cfg);

    xdg.portal = {
      enable = lib.mkDefault true;
      extraPortals = [ cfg.package ];
    };

    warnings = lib.optional (cfg.portals.print.enable && !config.services.printing.enable) ''
      services.xdg-desktop-portal-zenzai.portals.print requires CUPS, enable services.printing.
    '';
  };
}
