{
  enable = true;
  terminal = "wezterm";
  iconTheme = "Papirus";

  portals.account = {
    enable = true;
    command = "zenzai-dialog-access";
    id = "zenzai";
    name = "Zenzai User";
    image = "/home/zenzai/Pictures/avatar.png";
  };

  portals.access = {
    enable = true;
    command = "zenzai-dialog-access";
  };

  portals.usb = {
    enable = true;
    command = "zenzai-dialog-access";
  };

  portals.settings = {
    enable = true;
    colorScheme = "dark";
    accentColor = "#b4befe";
    contrast = "high";
    dict."org.gnome.SomeApp" = {
      some-key = "some value";
      some-number = 1337;
    };
  };

  portals.appchooser = {
    enable = true;
    command = "zenzai-app-select";
    writeMimeapps = false;
    defaults = {
      "text/plain" = {
        command = "ghostty";
        arguments = [
          "-e"
          "nvim"
        ];
      };
      "image/jpeg" = "io.github.woelper.Oculante";
      "image/*" = [
        "io.github.woelper.Oculante.desktop"
        {
          command = "imv";
        }
      ];
    };
  };

  portals.dynamicLauncher = {
    enable = true;
    command = "zenzai-dialog-launcher";
  };

  portals.email = {
    enable = true;
    command = "zenzai-app-select";
  };

  portals.print = {
    enable = true;
    command = "zenzai-dialog-print";
  };

  portals.notification.enable = true;

  theme = {
    name = "catppuccin-mocha";
    font = "Inter";
    textSize = 14;
    palette = {
      background = "#1e1e2e";
      text = "#cdd6f4";
      primary = "#b4befe";
      success = "#a6e3a1";
      warning = "#f9e2af";
      danger = "#f38ba8";
    };
  };
}
