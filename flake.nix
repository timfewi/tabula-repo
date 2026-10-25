{
  description = "Portable, source-bound repository context without a service";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/eaad089433ca2bb662274377d33df3d0e51ef28b";
    project-check = {
      url = "github:timfewi/project-check-nix/f70de45d69b9ca9a31f5b9f94e316ac6e43e514e";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { nixpkgs, project-check, ... }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          source = pkgs.lib.fileset.toSource {
            root = ./.;
            fileset = pkgs.lib.fileset.unions [
              ./tabula.py
              (pkgs.lib.fileset.fileFilter (file: file.hasExt "py") ./tabula)
            ];
          };
          tabula = pkgs.writeShellApplication {
            name = "tabula";
            runtimeInputs = [ pkgs.python3 ];
            text = ''
              exec python3 ${source}/tabula.py "$@"
            '';
          };
        in
        {
          inherit tabula;
          default = tabula;
        }
      );
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              project-check.packages.${system}.project-check
              pkgs.bashInteractive
              pkgs.coreutils
              pkgs.deadnix
              pkgs.git
              pkgs.jq
              pkgs.just
              pkgs.nix
              pkgs.nixfmt
              pkgs.python3
              pkgs.ripgrep
              pkgs.ruff
              pkgs.statix
            ];
          };
        }
      );
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt-tree);
    };
}
