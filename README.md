# xdg-desktop-portal-zenzai

A collection of several xdg-desktop-portal implementations to serve more lightweight wayland compositors like [Hyprland](https://hyprland.org/)

## Supported Portals

- [App Chooser](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.AppChooser.html) - choose an application
- [Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html) - control color scheme, accent color and appearance

## Install

Available via:

- nix: [atomicptr/nix](https://github.com/atomicptr/nix) via **pkgs.atomicptr.xdg-desktop-portal-zenzai**
- cargo: [xdg-desktop-portal-zenzai](https://crates.io/crates/xdg-desktop-portal-zenzai) (use meson to properly install everything)

## Configuration

Edit `$XDG_CONFIG_HOME/xdg-desktop-portal-zenzai/config.toml`

```toml
# define your terminal here, this will be used by some services (AppChooser only right now)
terminal = "ghostty"

### App Chooser Portal Config
[appchooser]
enabled = true

[appchooser.runner]
type = "dmenu" # currently only dmenu style API is supported (list of files into stdin)
command = "wofi"
arguments = ["--dmenu"]

# list of content types -> app associations
[appchooser.defaults]
"text/plain" = { command = "ghostty", arguments = ["-e", "nvim"] } # run arbitrary commands
"image/jpeg" = "io.github.woelper.Oculante" # or execute desktop files
"image/webp" = ["io.github.woelper.Oculante.desktop", "com.brave.Browser.desktop"] # you can also always pick from a group

### Settings Portal Config
[settings]
enabled = true # portals have to be explicitly enabled
color-scheme = "dark" # set color scheme to dark/light
accent-color = "#b4befe" # define an accent color
```

## How to use it

To use zenzai you need to create `~/.config/xdg-desktop-portal/CURRENT_DESKTOP_NAME-portals.conf`, for example, if you use Hyprland, you need to name it `Hyprland-portals.conf`.

```
[preferred]
default=hyprland;zenzai;gtk
org.freedesktop.impl.portal.AppChooser=zenzai
org.freedesktop.impl.portal.Settings=zenzai
```

## Motivation

The goal for me is to use this to replace xdg-desktop-portal-gtk completely on my Hyprland setup.

## License

GPLv3
