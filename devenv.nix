{ pkgs, lib, config, inputs, ... }:

let
  pkgs-playwright = import inputs.nixpkgs-playwright { system = pkgs.stdenv.system; };
  browsers = (builtins.fromJSON (builtins.readFile "${pkgs-playwright.playwright-driver}/browsers.json")).browsers;
  chromium-rev = (builtins.head (builtins.filter (x: x.name == "chromium") browsers)).revision;
in {
  env = {
    PLAYWRIGHT_BROWSERS_PATH = "${pkgs-playwright.playwright.browsers}";
    PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = true;
    PLAYWRIGHT_NODEJS_PATH = "${pkgs.nodejs}/bin/node";
    PLAYWRIGHT_LAUNCH_OPTIONS_EXECUTABLE_PATH = "${pkgs-playwright.playwright.browsers}/chromium-${chromium-rev}/chrome-linux/chrome";
    GREET = "devenv";
  };
    
  cachix.enable = false;

  packages = with pkgs; [
    cmake
    pkg-config
    pkgsStatic.openssl.dev
    pkgsStatic.openssl
    libudev-zero
    vscode-langservers-extracted
    typescript-language-server
    bashInteractive
    cargo-flamegraph
    just
    nodejs
  ];

  languages.javascript.enable = true;

  languages.rust = {
    enable = true;
    channel = "stable";
  };

  enterShell = ''
  '';
}
