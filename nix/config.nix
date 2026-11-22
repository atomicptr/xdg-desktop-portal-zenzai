{ lib }:
cfg:
let
  inherit (cfg.portals) settings appchooser print;
in
lib.filterAttrsRecursive (_: value: value != null) {
  inherit (cfg) terminal;

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

  print = {
    enabled = print.enable;
    command =
      if print.command != null then print.command else lib.getExe' cfg.package "zenzai-dialog-print";
  };

  theme = {
    inherit (cfg.theme) name font palette;
    text-size = cfg.theme.textSize;
  };
}
