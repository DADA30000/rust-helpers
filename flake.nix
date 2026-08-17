{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs supportedSystems (system: f system);
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "system-ui-helpers";
            version = "1.0.0";
            src = ./.;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.wrapGAppsHook4
            ];

            buildInputs = [
              pkgs.gtk4
              pkgs.gdk-pixbuf
              pkgs.glib
              pkgs.pango
              pkgs.cairo
            ];
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            nativeBuildInputs = [ pkgs.pkg-config ];

            buildInputs = [
              pkgs.cargo
              pkgs.rustc
              pkgs.rust-analyzer
              pkgs.gtk4
              pkgs.gdk-pixbuf
              pkgs.glib
              pkgs.pango
              pkgs.cairo
            ];
          };
        }
      );
    };
}
