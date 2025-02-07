{ pkgs, lib, config, inputs, ... }:

let
  openssl = pkgs.openssl.override {
      static = true;
  };
in {
  env.GREET = "devenv";
  env.PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
  env.LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [ openssl ];
  env.OPENSSL_DIR = "${pkgs.openssl.dev}";
  env.OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}";
  env.OPENSSL_NO_VENDOR = 1;
  env.OPENSSL_LIB_DIR = "${pkgs.lib.getLib openssl}/lib";

  packages = with pkgs; [
    cmake
    pkg-config
    openssl.dev
    openssl
    libudev-zero
    vscode-langservers-extracted
    typescript-language-server
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    targets = [ "x86_64-pc-windows-msvc" ];
  };

  enterShell = ''
  '';

  # https://devenv.sh/tasks/
  # tasks = {
  #   "myproj:setup".exec = "mytool build";
  #   "devenv:enterShell".after = [ "myproj:setup" ];
  # };
}
