{
  description = "A post-modern text editor.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    ...
  }: let
    inherit (nixpkgs) lib;
    eachSystem = lib.genAttrs lib.systems.flakeExposed;
    pkgsFor = eachSystem (system:
      import nixpkgs {
        localSystem.system = system;
        overlays = [(import rust-overlay) self.overlays.helix];
      });
    gitRev = self.rev or self.dirtyRev or null;
    gpuiSystems = ["x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin"];
  in {
    packages = eachSystem (system:
      {
        inherit (pkgsFor.${system}) helix;
        /*
        The default Helix build. Uses the latest stable Rust toolchain, and unstable
        nixpkgs.

        The build inputs can be overridden with the following:

        packages.${system}.default.override { rustPlatform = newPlatform; };

        Overriding a derivation attribute can be done as well:

        packages.${system}.default.overrideAttrs { buildType = "debug"; };
        */
        default =
          if builtins.elem system gpuiSystems
          then self.packages.${system}.helix-gpui
          else self.packages.${system}.helix;
      }
      // lib.optionalAttrs (builtins.elem system gpuiSystems) {
        inherit (pkgsFor.${system}) helix-gpui;
      });
    apps = eachSystem (system:
      {
        default = {
          type = "app";
          program = lib.getExe self.packages.${system}.default;
        };
        helix = {
          type = "app";
          program = lib.getExe self.packages.${system}.helix;
        };
      }
      // lib.optionalAttrs (builtins.elem system gpuiSystems) {
        helix-gpui = {
          type = "app";
          program = lib.getExe self.packages.${system}.helix-gpui;
        };
      });
    checks = lib.mapAttrs (system: pkgs: let
      # Get Helix's MSRV toolchain to build with by default.
      msrvToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      msrvPlatform = pkgs.makeRustPlatform {
        cargo = msrvToolchain;
        rustc = msrvToolchain;
      };
    in
      {
        helix = self.packages.${system}.helix.override {
          rustPlatform = msrvPlatform;
        };
      }
      // lib.optionalAttrs (builtins.elem system gpuiSystems) {
        helix-gpui = self.packages.${system}.helix-gpui;
      })
    pkgsFor;

    # Include the default frontend's native build dependencies in the dev shell.
    devShells =
      lib.mapAttrs (system: pkgs: {
        default = let
          commonRustFlagsEnv = "-C link-arg=-fuse-ld=lld -C target-cpu=native --cfg tokio_unstable";
          platformRustFlagsEnv = lib.optionalString pkgs.stdenv.isLinux "-Clink-arg=-Wl,--no-rosegment";
        in
          pkgs.mkShell {
            inputsFrom = [
              (self.packages.${system}.default.override {
                includeGrammarIf = _: false;
              })
            ];
            nativeBuildInputs = with pkgs;
              [
                lld
                cargo-flamegraph
                rust-bin.nightly.latest.rust-analyzer
                mdbook
              ]
              ++ (lib.optional (stdenv.isx86_64 && stdenv.isLinux) cargo-tarpaulin)
              ++ (lib.optional stdenv.isLinux lldb);
            shellHook = ''
              export RUST_BACKTRACE="1"
              export RUSTFLAGS="''${RUSTFLAGS:-""} ${commonRustFlagsEnv} ${platformRustFlagsEnv}"
            '';
          };
      })
      pkgsFor;

    overlays = {
      helix = final: prev: {
        helix = final.callPackage ./default.nix {inherit gitRev;};
        helix-gpui = final.callPackage ./helix-gpui/package.nix {inherit gitRev;};
      };

      default = self.overlays.helix;
    };
  };
  nixConfig = {
    extra-substituters = ["https://helix.cachix.org"];
    extra-trusted-public-keys = ["helix.cachix.org-1:ejp9KQpR1FBI2onstMQ34yogDm4OgU2ru6lIwPvuCVs="];
  };
}
