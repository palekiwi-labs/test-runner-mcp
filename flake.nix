{
  description = "A Rust flake for rspec-mcp";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, fenix, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      rustToolchain = fenix.packages.${system}.stable.toolchain;
    in
    {
      devShells.${system}.default = pkgs.mkShell
        {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.rust-analyzer
            pkgs.cargo-expand
            pkgs.cargo-watch
            pkgs.cargo-edit

          ];

          # Set environment variables for OpenSSL
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          OPENSSL_DIR = "${pkgs.openssl.out}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

          shellHook = ''
            echo "Rust development environment ready!"
            echo "Rust version: $(rustc --version)"
          '';
        };
    };
}
