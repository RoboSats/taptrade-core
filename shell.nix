{ pkgs ? import <nixpkgs> {} }:
  pkgs.mkShell {
    nativeBuildInputs = with pkgs.buildPackages; [ 
        pkg-config 
        zlib 
        openssl 
        cargo 
        rustc 
        rustfmt
        gcc
     ];
     RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
}

# For VSCode and Rust Analyzer use the Extension "Nix Environment Selector"