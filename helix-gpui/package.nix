{
  lib,
  stdenv,
  callPackage,
  rustPlatform,
  pkg-config,
  clang,
  libclang,
  makeWrapper,
  openssl,
  fontconfig,
  freetype,
  libxkbcommon,
  wayland,
  vulkan-loader,
  libGL,
  libx11,
  libxcb,
  libxcursor,
  libxi,
  libxrandr,
  gitRev ? null,
  grammarOverlays ? [],
  includeGrammarIf ? _: true,
}: let
  helix = callPackage ../default.nix {
    inherit rustPlatform gitRev grammarOverlays includeGrammarIf;
  };
  linuxLibraries = [
    fontconfig
    freetype
    libxkbcommon
    wayland
    vulkan-loader
    libGL
    libx11
    libxcb
    libxcursor
    libxi
    libxrandr
  ];
in
  helix.overrideAttrs (old: {
    name = "helix-gpui";
    cargoBuildFlags = ["-p" "helix-gpui"];
    nativeBuildInputs =
      old.nativeBuildInputs
      ++ [pkg-config clang makeWrapper]
      ++ lib.optionals stdenv.isDarwin [rustPlatform.bindgenHook];
    buildInputs =
      (old.buildInputs or [])
      ++ [openssl]
      ++ lib.optionals stdenv.isLinux linuxLibraries;
    env = old.env // {LIBCLANG_PATH = "${libclang.lib}/lib";};
    postInstall =
      ''
        mkdir -p $out/share/applications $out/share/icons/hicolor/scalable/apps
        cp ${../logo.svg} $out/share/icons/hicolor/scalable/apps/helix.svg
        cat > $out/share/applications/Helix-GPUI.desktop <<EOF
        [Desktop Entry]
        Type=Application
        Name=Helix GPUI
        Comment=Native graphical Helix editor
        Exec=hx-gpui %F
        Icon=helix
        Terminal=false
        Categories=Development;TextEditor;
        MimeType=text/plain;
        EOF
      ''
      + lib.optionalString stdenv.isLinux ''
        wrapProgram $out/bin/hx-gpui \
          --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath linuxLibraries}
      '';
    meta =
      (old.meta or {})
      // {
        mainProgram = "hx-gpui";
        description = "Helix editor with a native GPUI desktop frontend";
        platforms = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
      };
  })
