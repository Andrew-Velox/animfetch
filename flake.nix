{
  description = "An animated system fetch you can work inside";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          animfetch = pkgs.rustPlatform.buildRustPackage {
            pname = "animfetch";
            version = "0.1.5";

            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            meta = {
              description = "Animated system information fetch tool";
              homepage = "https://github.com/Andrew-Velox/animfetch";
              license = pkgs.lib.licenses.mit;
              mainProgram = "animfetch";
            };
          };

          default = self.packages.${system}.animfetch;
        }
      );

      apps = forAllSystems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.animfetch}/bin/animfetch";
        };
      });
    };
}
