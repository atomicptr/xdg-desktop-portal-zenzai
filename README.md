# xdg-desktop-portal-zenzai

A collection of several xdg-desktop-portal implementations to serve more lightweight wayland compositors like [Hyprland](https://hyprland.org/)

## Supported Portals

- [Access](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Access.html) - approve or deny access requests
- [App Chooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.AppChooser.html) - choose an application
- [Dynamic Launcher](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.DynamicLauncher.html) - approve launcher creation
- [Email](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Email.html) - compose email using the application chooser
- [Notification](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Notification.html) - forward notifications to a local daemon such as mako or dunst
- [Print](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Print.html) - print documents (requires CUPS)
- [Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html) - control color scheme, accent color and appearance
- [USB](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Usb.html) - approve USB device access requests

## Tools

Tools integrated with the portal

- `zenzai-app-select` - Custom app chooser dialog
- `zenzai-dialog-access` - Access confirmation dialog
- `zenzai-dialog-launcher` - Custom launcher approval dialog
- `zenzai-dialog-print` - Custom print dialog

## Install

### NixOS (flakes)

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    zenzai = {
      url = "github:atomicptr/xdg-desktop-portal-zenzai";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, zenzai, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix
        zenzai.nixosModules.default
        {
          services.xdg-desktop-portal-zenzai = {
            enable = true;
            portals.settings = {
              colorScheme = "dark";
              accentColor = "#b4befe";
            };
          };

          xdg.portal.config.Hyprland."org.freedesktop.impl.portal.Settings" = [ "zenzai" ];
        }
      ];
    };
  };
}
```

## Configuration

Edit `$XDG_CONFIG_HOME/xdg-desktop-portal-zenzai/config.toml`

```toml
# define your terminal here, this will be used by some services (AppChooser only right now)
terminal = "ghostty"
icon-theme = "Papirus"

### Access Portal Config (opt-in)
[access]
enabled = true
command = "zenzai-dialog-access" # executable override, looked up on PATH by default

### USB Portal Config (opt-in)
[usb]
enabled = true # independent of access.enabled
command = "zenzai-dialog-access" # executable override, looked up on PATH by default

### App Chooser Portal Config
[appchooser]
enabled = true
command = "zenzai-app-select" # path to the `zenzai-app-select` cli, looked up on PATH by default
write-mimeapps = true # set false to keep remembered defaults local without updating mimeapps.list

# list of content types -> app associations
[appchooser.defaults]
"text/plain" = { command = "ghostty", arguments = ["-e", "nvim", "--", "%file%"] } # run arbitrary commands
"image/jpeg" = "io.github.woelper.Oculante" # or execute desktop files
"image/*" = ["io.github.woelper.Oculante.desktop", { command = "imv" }]

### Dynamic Launcher Portal Config (opt-in)
[dynamic-launcher]
enabled = true
command = "zenzai-dialog-launcher" # executable override, looked up on PATH by default

### Email Portal Config
[email]
enabled = true # independent of appchooser.enabled
command = "zenzai-app-select" # selector executable override, looked up on PATH by default

### Settings Portal Config
[settings]
enabled = true # default; set false to disable
color-scheme = "dark" # set color scheme to dark/light
accent-color = "#b4befe" # define an accent color

#### some apps query arbitrary data from the settings portal which you can set like this:
[settings.dict."org.gnome.SomeApp"]
some-key = "some value"
some-number = 1337

### Print Portal Config
[print]
enabled = true
command = "zenzai-dialog-print" # path to the `zenzai-dialog-print` cli, looked up on PATH by default

### Notification Portal Config
[notification]
enabled = true # requires a daemon providing org.freedesktop.Notifications

### Theme for internal tools like `zenzai-app-select` and `zenzai-dialog-print`
[theme]
name = "catppuccin-mocha" # any built-in iced theme, defaults to light/dark based on settings.color-scheme
font = "Inter"
text-size = 14

[theme.palette] # optional overrides of background, text, primary, success, warning, danger
primary = "#b4befe" # defaults to settings.accent-color
```

## How to use it

To use zenzai you need to create `~/.config/xdg-desktop-portal/CURRENT_DESKTOP_NAME-portals.conf`, for example, if you use Hyprland, you need to name it `Hyprland-portals.conf`.

```
[preferred]
default=hyprland;zenzai;gtk
org.freedesktop.impl.portal.Access=zenzai
org.freedesktop.impl.portal.AppChooser=zenzai
org.freedesktop.impl.portal.DynamicLauncher=zenzai
org.freedesktop.impl.portal.Email=zenzai
org.freedesktop.impl.portal.Settings=zenzai
org.freedesktop.impl.portal.Print=zenzai
org.freedesktop.impl.portal.Notification=zenzai
org.freedesktop.impl.portal.Usb=zenzai
```

## Motivation

The goal for me is to use this to replace xdg-desktop-portal-gtk completely on my Hyprland setup.

## License

GPLv3
