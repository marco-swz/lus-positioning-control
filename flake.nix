{
    description = "A very basic flake";

    inputs = {
        nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
        flake-utils.url = "github:numtide/flake-utils";
        rust-overlay.url = "github:oxalica/rust-overlay";
    };

    outputs = { nixpkgs, flake-utils, rust-overlay,  ... }:
        flake-utils.lib.eachSystem [ "x86_64-linux" ] (system: 
            let
                pkgs = import nixpkgs {
                    inherit system;
                    overlays = [
                        rust-overlay.overlays.default
                    ];
                };
                rust-bin = pkgs.rust-bin.stable.latest.default.override {
                  extensions = [ "rust-src" ];
                  targets = [ "x86_64-pc-windows-msvc" ];
                };

            in {
                devShell = pkgs.mkShell {
                    PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
                    LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.openssl ];
                    OPENSSL_DIR = "${pkgs.openssl.dev}";
                    nativeBuildInputs = [ pkgs.pkg-config ];
                    buildInputs = with pkgs; [ 
                        cmake
                        rust-analyzer
                        pkg-config
                        openssl.dev
                        openssl
                        rust-bin
                        rustup
                        libudev-zero
                        vscode-langservers-extracted
                        typescript-language-server
                    ];
                    shellHook = ''
                        export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig
                        export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath [ pkgs.openssl ]}
                        export OPENSSL_DIR="${pkgs.openssl.dev}"
                    '';
                };
            });
}
