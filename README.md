# xdg-desktop-portal-zenzai

A collection of several xdg-desktop-portal implementations to serve more lightweight wayland compositors like [Hyprland](https://hyprland.org/)

## Supported Portals

- [Settings](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Settings.html) - control color scheme, accent color and appearance

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
