{
  description = "A typing pet overlay for Linux Wayland";

  nixConfig = {
    extra-substituters = [ "https://pachipachi.cachix.org" ];
    extra-trusted-public-keys = [
      "pachipachi.cachix.org-1:RLf57/pbi0GJh6SBUEM4HdDSYI4lKNR0OzZp43qkeTw="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane.url = "github:ipetkov/crane";
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      crane,
      ...
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      pkgsFor = forAllSystems (
        system:
        import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        }
      );
      toolchainFor = system: pkgsFor.${system}.rust-bin.stable.latest.minimal;
      librariesFor =
        system: with pkgsFor.${system}; [
          wayland
          libxkbcommon
          fontconfig
          freetype
          openssl
          vulkan-loader
          libGL
          stdenv.cc.cc.lib
        ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          craneLib = (crane.mkLib pkgs).overrideToolchain (toolchainFor system);
          manifest = fromTOML (builtins.readFile ./Cargo.toml);
          commonArgs = {
            pname = manifest.package.name;
            version = manifest.package.version;
            src = pkgs.lib.fileset.toSource {
              root = ./.;
              fileset = pkgs.lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./src
              ];
            };
            strictDeps = true;
            nativeBuildInputs = with pkgs; [
              pkg-config
              cmake
            ];
            buildInputs = librariesFor system;
          };
          cargoArtifacts = craneLib.buildDepsOnly commonArgs;
          pachipachi = craneLib.buildPackage (
            commonArgs
            // {
              inherit cargoArtifacts;
              nativeBuildInputs = commonArgs.nativeBuildInputs ++ [ pkgs.makeWrapper ];
              postFixup = ''
                wrapProgram "$out/bin/pachipachi" \
                  --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath (librariesFor system)}"
              '';
              meta = {
                description = manifest.package.description;
                mainProgram = "pachipachi";
                platforms = systems;
              };
            }
          );
        in
        {
          inherit pachipachi;
          default = pachipachi;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
          libraries = librariesFor system;
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              ((toolchainFor system).override {
                extensions = [
                  "rustfmt"
                  "clippy"
                  "rust-src"
                  "rust-analyzer"
                ];
              })
              pkg-config
              cmake
              clang
              mold
            ];
            buildInputs = libraries;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath libraries;
          };
        }
      );
    };
}
