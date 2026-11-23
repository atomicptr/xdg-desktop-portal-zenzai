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

  command = types.submodule {
    options = {
      command = mkOption {
        type = types.str;
        description = "Executable to run.";
      };

      arguments = mkOption {
        type = types.listOf types.str;
        default = [ ];
        description = ''
          Arguments without shell expansion or word splitting. Each `%file%` is replaced
          once with the local path or non-local URI, including in `--input=%file%`.
          Without a placeholder, the file is appended as the last argument.
        '';
      };
    };
  };
  target = types.either types.str command;
in
{
  enable = mkEnableOption "xdg-desktop-portal-zenzai";

  debug = mkEnableOption "debug logging for the portal and companion tools";

  package = mkOption {
    type = types.package;
    default = defaultPackage;
    description = "The xdg-desktop-portal-zenzai package to use.";
  };

  terminal = nullOr types.str ''
    Terminal used by some portals, e.g. alacritty, foot, ghostty, kitty, st, wezterm or any other
    executable accepting `-e <command>`. Detected automatically when unset.
  '';

  iconTheme = nullOr types.str "Icon theme used by all companion GUIs; preserves default lookup when unset.";
  portals = {
    account = {
      enable = (mkEnableOption "the Account portal") // {
        default = true;
      };

      command = nullOr types.str "Account approval dialog executable, defaults to the packaged zenzai-dialog-access.";

      id = nullOr types.str ''
        Account ID shared after approval. Defaults to the system login username when unset;
        an empty string suppresses the ID. This value is not a secret.
      '';

      name = nullOr types.str ''
        Account name shared after approval. Defaults to the GECOS real name when unset;
        an empty string suppresses the name. This value is not a secret.
      '';

      image = nullOr (types.strMatching "/.*") ''
        Absolute local path to the account image shared after approval. No image is
        discovered automatically. The dialog shows the path, not an image preview.
        This value is not a secret.
      '';
    };

    access = {
      enable = (mkEnableOption "the Access portal") // {
        default = true;
      };

      command = nullOr types.str "Access dialog executable, defaults to the packaged zenzai-dialog-access.";
    };

    usb = {
      enable = (mkEnableOption "the USB portal") // {
        default = true;
      };

      command = nullOr types.str "USB approval dialog executable, defaults to the packaged zenzai-dialog-access.";
    };

    settings = {
      enable = (mkEnableOption "the Settings portal") // {
        default = true;
      };

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
      enable = (mkEnableOption "the App Chooser portal") // {
        default = true;
      };

      command = nullOr types.str "Application selector executable, defaults to the packaged zenzai-app-select.";

      writeMimeapps = mkOption {
        type = types.bool;
        default = true;
        description = ''
          Whether setting a default application also updates an existing writable user
          mimeapps.list. Disabling this leaves MIME default lookup and local remembering enabled.
          Missing files are never created and managed symlinks are never modified.
        '';
      };

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

    dynamicLauncher = {
      enable = (mkEnableOption "the DynamicLauncher portal") // {
        default = true;
      };

      command = nullOr types.str "Launcher dialog executable, defaults to the packaged zenzai-dialog-launcher.";
    };

    email = {
      enable = (mkEnableOption "the Email portal") // {
        default = true;
      };

      command = nullOr types.str "Email application selector executable, defaults to the packaged zenzai-app-select.";
    };

    print = {
      enable = (mkEnableOption "the Print portal") // {
        default = true;
      };

      command = nullOr types.str "Print dialog executable, defaults to the packaged zenzai-dialog-print.";
    };

    notification.enable =
      (mkEnableOption "the Notification portal (requires a local notification daemon)")
      // {
        default = true;
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
