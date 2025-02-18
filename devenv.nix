{ pkgs, lib, config, inputs, ... }:

let
  #openssl = pkgs.openssl.override {
   #   static = true;
  #};
in {
  env.GREET = "devenv";
  # env.PKG_CONFIG_PATH = "${pkgs.pkgsStatic.openssl.dev}/lib/pkgconfig";
  # env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ pkgs.pkgsStatic.openssl ];
  # env.OPENSSL_DIR = "${pkgs.pkgsStatic.openssl.dev}";
  # env.OPENSSL_INCLUDE_DIR = "${pkgs.pkgsStatic.openssl.dev}/include";
  # env.OPENSSL_LIB_DIR = "${pkgs.lib.getLib pkgs.pkgsStatic.openssl}/lib";

  packages = with pkgs; [
    cmake
    pkg-config
    pkgsStatic.openssl.dev
    pkgsStatic.openssl
    libudev-zero
    vscode-langservers-extracted
    typescript-language-server
    bashInteractive
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
  };

  enterShell = ''
  '';
}
