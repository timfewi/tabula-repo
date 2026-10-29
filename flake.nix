{
  description = "Tabula: offline repository guides with source freshness checks";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/eaad089433ca2bb662274377d33df3d0e51ef28b";
  outputs =
    { nixpkgs, ... }:
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
          mkTabula =
            buildPkgs:
            buildPkgs.rustPlatform.buildRustPackage {
              pname = "tabula";
              version = "0.1.0";
              src = pkgs.lib.fileset.toSource {
                root = ./.;
                fileset = pkgs.lib.fileset.unions [
                  ./Cargo.toml
                  ./Cargo.lock
                  ./src
                  (pkgs.lib.fileset.fileFilter (
                    file: file.hasExt "rs" || file.hasExt "json" || file.hasExt "sha256"
                  ) ./tests)
                  ./README.md
                  ./docs/reference.md
                  ./LICENSE
                  ./THIRD_PARTY_LICENSES
                ];
              };
              cargoLock.lockFile = ./Cargo.lock;
              postInstall = ''
                install -Dm644 README.md "$out/share/doc/tabula/README.md"
                install -Dm644 docs/reference.md "$out/share/doc/tabula/REFERENCE.md"
                install -Dm644 LICENSE "$out/share/doc/tabula/LICENSE"
                install -Dm644 THIRD_PARTY_LICENSES "$out/share/doc/tabula/THIRD_PARTY_LICENSES"
              '';
              meta = {
                description = "Offline repository guides with source freshness checks";
                license = pkgs.lib.licenses.mit;
                mainProgram = "tabula";
                platforms = systems;
              };
            };
          tabula = mkTabula pkgs;
        in
        {
          inherit tabula;
          default = tabula;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          tabula-static = mkTabula pkgs.pkgsStatic;
        }
      );
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              just
              nodejs
              bun
              nixfmt
              shellcheck
            ];
          };
        }
      );
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}
