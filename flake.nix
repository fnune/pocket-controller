{
  description = "pocket-controller: use a Linux handheld as a wired USB controller";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f (
            import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            }
          )
        );
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.rustPlatform.buildRustPackage {
          pname = "pocket-controller";
          version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          meta.mainProgram = "pocket-controller";
        };
      });

      checks = nixpkgs.lib.genAttrs [ "x86_64-linux" ] (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          vm = import ./nix/vm-test.nix {
            inherit pkgs;
            pocket-controller = self.packages.${system}.default;
          };
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            (pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
            pkgs.android-tools
            pkgs.curl
            pkgs.just
            pkgs.usbutils
            pkgs.zip
            pkgs.imagemagick
            pkgs.resvg
            pkgs.cargo-zigbuild
            pkgs.zig
            pkgs.python3
          ];
        };
      });
    };
}
