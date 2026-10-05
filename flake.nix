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

            RUSTFLAGS = builtins.concatStringsSep " " [
              "-D warnings"
              "-D dead-code"
              "-D unused-imports"
              "-W rust-2024-compatibility"
              "-W future-incompatible"
              "-W nonstandard-style"
              "-C overflow-checks=on"
              "-C link-arg=-Wl,-z,relro,-z,now"
              "-C link-arg=-Wl,-z,noexecstack"
            ];

            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.wrapGAppsHook4
              pkgs.clippy
              pkgs.rustfmt
            ];

            preBuild = ''
              echo "Checking formatting with rustfmt (system-ui-helpers)..."
              rustfmt --edition 2024 --check src/lib.rs src/bin/*.rs

              echo "Running exhaustive clippy hardening analysis (system-ui-helpers)..."
              cargo clippy --all-targets -- \
                -D warnings \
                -D dead-code \
                -D unused-imports \
                -W clippy::all \
                -W clippy::correctness \
                -W clippy::suspicious \
                -W clippy::complexity \
                -W clippy::perf \
                -W clippy::style \
                -W clippy::pedantic \
                -W clippy::nursery \
                -W clippy::clone_on_ref_ptr \
                -W clippy::dbg_macro \
                -W clippy::todo \
                -W clippy::unimplemented \
                -W clippy::rc_buffer \
                -W clippy::mutex_atomic \
                -W clippy::mem_forget \
                -W clippy::manual_assert \
                -W rust-2024-compatibility \
                -W future-incompatible \
                -W nonstandard-style
            '';

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
            nativeBuildInputs = [
              pkgs.pkg-config
              pkgs.clippy
              pkgs.rustfmt
            ];

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
