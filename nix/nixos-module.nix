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
    environment.etc."xdg/xdg-desktop-portal-zenzai/config.toml".source =
      (pkgs.formats.toml { }).generate "config.toml"
        (import ./config.nix { inherit lib; } cfg);

    xdg.portal = {
      enable = lib.mkDefault true;
      extraPortals = [ cfg.package ];
    };

    systemd.user.services.xdg-desktop-portal-zenzai = lib.mkIf cfg.debug {
      overrideStrategy = "asDropin";

      enableDefaultPath = false;
      environment = {
        RUST_LOG = "warn,xdg_desktop_portal_zenzai=debug,zenzai_core=debug,zenzai_app_select=debug,zenzai_dialog_print=debug,zenzai_dialog_launcher=debug";
      };
    };

    warnings = lib.optional (cfg.portals.print.enable && !config.services.printing.enable) ''
      services.xdg-desktop-portal-zenzai.portals.print requires CUPS, enable services.printing.
    '';
  };
}
