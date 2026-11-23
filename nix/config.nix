{ lib }:
cfg:
let
  inherit (cfg.portals)
    access
    settings
    appchooser
    dynamicLauncher
    email
    print
    ;
in
lib.filterAttrsRecursive (_: value: value != null) {
  inherit (cfg) terminal;
  icon-theme = cfg.iconTheme;

  access = {
    enabled = access.enable;
    command =
      if access.command != null then access.command else lib.getExe' cfg.package "zenzai-dialog-access";
  };

  settings = {
    enabled = settings.enable;
    color-scheme = settings.colorScheme;
    accent-color = settings.accentColor;
    contrast = settings.contrast;
    inherit (settings) dict;
  };

  appchooser = {
    enabled = appchooser.enable;
    write-mimeapps = appchooser.writeMimeapps;
    command =
      if appchooser.command != null then
        appchooser.command
      else
        lib.getExe' cfg.package "zenzai-app-select";
    inherit (appchooser) defaults;
  };

  dynamic-launcher = {
    enabled = dynamicLauncher.enable;
    command =
      if dynamicLauncher.command != null then
        dynamicLauncher.command
      else
        lib.getExe' cfg.package "zenzai-dialog-launcher";
  };

  email = {
    enabled = email.enable;
    command =
      if email.command != null then email.command else lib.getExe' cfg.package "zenzai-app-select";
  };

  print = {
    enabled = print.enable;
    command =
      if print.command != null then print.command else lib.getExe' cfg.package "zenzai-dialog-print";
  };

  notification.enabled = cfg.portals.notification.enable;

  theme = {
    inherit (cfg.theme) name font palette;
    text-size = cfg.theme.textSize;
  };
}
