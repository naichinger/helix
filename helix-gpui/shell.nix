# Standalone GPUI development environment on NixOS.
let
  pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/archive/56c02bc00adcf003215cc4bd996d6efaf4cff188.tar.gz") {};
in pkgs.mkShell {
  packages = with pkgs; [ rustc cargo rustfmt clippy git pkg-config clang libclang ];
  buildInputs = with pkgs; [ wayland libxkbcommon fontconfig freetype vulkan-loader openssl libx11 libxcb libxcursor libxi libxrandr ];
  LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [ vulkan-loader libxkbcommon wayland libGL fontconfig freetype libx11 libxcb ]);
}
