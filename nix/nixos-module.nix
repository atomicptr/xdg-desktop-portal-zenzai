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

    systemd.user.services.xdg-desktop-portal-zenzai = lib.mkIf cfg.debug {
      # Keep the session PATH used to resolve user-configured applications.
      enableDefaultPath = false;
      environment.RUST_LOG = "warn,xdg_desktop_portal_zenzai=debug,zenzai_core=debug,zenzai_app_select=debug,zenzai_dialog_print=debug";
    };

    warnings = lib.optional (cfg.portals.print.enable && !config.services.printing.enable) ''
      services.xdg-desktop-portal-zenzai.portals.print requires CUPS, enable services.printing.
    '';
  };
}
