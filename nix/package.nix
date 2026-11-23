{
  lib,
  cups,
  libxkbcommon,
  makeWrapper,
  pkg-config,
  rustPlatform,
  wayland,
  xdg-utils,
}:
let
  cargoToml = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "xdg-desktop-portal-zenzai";
  inherit (cargoToml.workspace.package) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../share
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    makeWrapper
    pkg-config
    rustPlatform.bindgenHook
  ];

  buildInputs = [ cups ];

  postInstall = ''
    # the portal is started by D-Bus activation, not by users
    mkdir -p $out/libexec
    mv $out/bin/xdg-desktop-portal-zenzai $out/libexec/

    install -Dm644 share/zenzai.portal -t $out/share/xdg-desktop-portal/portals
    mkdir -p $out/share/dbus-1/services $out/share/systemd/user

    substitute share/org.freedesktop.impl.portal.desktop.zenzai.service.in \
      $out/share/dbus-1/services/org.freedesktop.impl.portal.desktop.zenzai.service \
      --replace-fail @xdg_install_dir@ $out/libexec

    substitute share/xdg-desktop-portal-zenzai.service.in \
      $out/share/systemd/user/xdg-desktop-portal-zenzai.service \
      --replace-fail @xdg_install_dir@ $out/libexec
  '';

  # winit loads these at runtime via dlopen
  postFixup = ''
    for executable in zenzai-dialog-print zenzai-app-select zenzai-dialog-launcher; do
      patchelf --add-rpath ${
        lib.makeLibraryPath [
          libxkbcommon
          wayland
        ]
      } "$out/bin/$executable"
    done
    wrapProgram "$out/bin/zenzai-app-select" \
      --prefix PATH : ${lib.makeBinPath [ xdg-utils ]}
  '';

  meta = {
    description = "A collection of several xdg-desktop-portal implementations to serve more lightweight wayland compositors like Hyprland";
    homepage = "https://github.com/atomicptr/xdg-desktop-portal-zenzai";
    license = lib.licenses.gpl3Plus;
    platforms = lib.platforms.linux;
    mainProgram = "zenzai-dialog-print";
  };
}
