{ lib, defaultPackage }:
let
  inherit (lib) mkEnableOption mkOption types;

  nullOr =
    type: description:
    mkOption {
      type = types.nullOr type;
      default = null;
      inherit description;
    };

  commandOptions = {
    command = mkOption {
      type = types.str;
      description = "Executable to run.";
    };

    arguments = mkOption {
      type = types.listOf types.str;
      default = [ ];
      description = ''
        Arguments passed to the executable. Each occurrence of `%file%` is replaced with the
        local file path, or the original URI for non-local resources, including within an
        argument such as `--input=%file%`. Without this placeholder, the file is appended
        as the last argument. Each list element remains one argument, with no shell expansion
        or word splitting, placeholder-like text in the file path is not expanded again.
      '';
    };
  };

  command = types.submodule { options = commandOptions; };
  target = types.either types.str command;
in
{
  enable = mkEnableOption "xdg-desktop-portal-zenzai";

  package = mkOption {
    type = types.package;
    default = defaultPackage;
    description = "The xdg-desktop-portal-zenzai package to use.";
  };

  terminal = nullOr types.str ''
    Terminal used by some portals, e.g. alacritty, foot, ghostty, kitty, st, wezterm or any other
    executable accepting `-e <command>`. Detected automatically when unset.
  '';

  portals = {
    settings = {
      enable = mkEnableOption "the Settings portal";

      colorScheme = nullOr (types.enum [
        "no-preference"
        "dark"
        "light"
      ]) "Preferred color scheme.";

      accentColor = nullOr types.str "Accent color as CSS color string.";

      contrast = nullOr (types.enum [
        "no-preference"
        "high"
      ]) "Preferred contrast.";

      dict = mkOption {
        type = types.attrsOf (
          types.attrsOf (
            types.oneOf [
              types.bool
              types.int
              types.float
              types.str
            ]
          )
        );
        default = { };
        example = {
          "org.gnome.SomeApp".some-key = "some value";
        };
        description = "Arbitrary values served by the Settings portal, grouped by namespace.";
      };
    };

    appchooser = {
      enable = mkEnableOption "the App Chooser portal";

      command = nullOr types.str "Application selector executable, defaults to the packaged zenzai-app-select.";

      defaults = mkOption {
        type = types.attrsOf (types.either target (types.listOf target));
        default = { };
        example = {
          "text/plain" = {
            command = "ghostty";
            arguments = [
              "-e"
              "nvim"
              "--"
              "%file%"
            ];
          };
          "image/jpeg" = "io.github.woelper.Oculante";
        };
        description = ''
          Content type to application mapping: a desktop file, a command, or a list of both to
          pick from. A single target launches immediately. Exact matches take priority over patterns
          ending in `*`, where the most specific pattern wins, then remembered exact matches.
        '';
      };
    };

    print = {
      enable = mkEnableOption "the Print portal";

      command = nullOr types.str "Print dialog executable, defaults to the packaged zenzai-dialog-print.";
    };
  };

  theme = {
    name = nullOr types.str ''
      Built-in theme such as "catppuccin-mocha" or "tokyo-night", follows colorScheme when unset.
    '';

    font = nullOr types.str "Font family used by GUI tools.";

    textSize = nullOr types.number "Default text size of GUI tools.";

    palette = lib.genAttrs [ "background" "text" "primary" "success" "warning" "danger" ] (
      name: nullOr types.str "Override for the ${name} color of the theme."
    );
  };
}
