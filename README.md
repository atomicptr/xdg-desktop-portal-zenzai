# xdg-desktop-portal-zenzai

A collection of several xdg-desktop-portal implementations to serve more lightweight wayland compositors like [Hyprland](https://hyprland.org/)

## Supported Portals

- [Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html) - control color scheme, accent color and appearance
- [App Chooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.AppChooser.html) - choose an application

## Install

Available via:

- nix: [atomicptr/nix](https://github.com/atomicptr/nix) via **pkgs.atomicptr.xdg-desktop-portal-zenzai**
- cargo: [xdg-desktop-portal-zenzai](https://crates.io/crates/xdg-desktop-portal-zenzai) (use meson to properly install everything)

## Configuration

Edit `$XDG_CONFIG_HOME/xdg-desktop-portal-zenzai/config.toml`

```toml
# configurations for the settings portal
[settings]
enabled = true # portals have to be explicitly enabled
color-scheme = "dark" # set color scheme to dark/light
accent-color = "#b4befe" # define an accent color

[appchooser]
enabled = true

[appchooser.runner]
type = "dmenu"
command = "wofi"
arguments = ["--dmenu"]

[appchooser.defaults]
"text/plain" = { command = "ghostty", arguments = ["-e", "nvim"] }
"image/jpeg" = "io.github.woelper.Oculante"
"image/webp" = "io.github.woelper.Oculante"
"image/png" = "io.github.woelper.Oculante"
"image/gif" = "io.github.woelper.Oculante"
```

## How to use it

To use zenzai you need to create `~/.config/xdg-desktop-portal/CURRENT_DESKTOP_NAME-portals.conf`, for example, if you use Hyprland, you need to name it `Hyprland-portals.conf`.

```
[preferred]
default=hyprland;zenzai;gtk
org.freedesktop.impl.portal.Settings=zenzai;gtk
```

## Motivation

The goal for me is to use this to replace xdg-desktop-portal-gtk completely on my Hyprland setup.

## License

GPLv3
