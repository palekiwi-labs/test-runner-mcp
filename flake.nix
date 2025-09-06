{
  description = "A Rust flake for test-runner-mcp";

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
      packages.${system}.default = pkgs.rustPlatform.buildRustPackage {
        pname = "test-runner-mcp";
        version = "0.1.0";
        src = ./.;
        
        cargoHash = "sha256-Wdo7dIlQfQGABeR1Kvd7096sLXnqn3YpZvXHlI41ULk=";
        
        nativeBuildInputs = with pkgs; [
          pkg-config
        ];
        
        buildInputs = with pkgs; [
          openssl
        ];
        
        # Set environment variables for OpenSSL
        PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        OPENSSL_DIR = "${pkgs.openssl.out}";
        OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
        OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        
        meta = with pkgs.lib; {
          description = "A Rust flake for test-runner-mcp";
          license = licenses.mit;
          maintainers = [ ];
        };
      };

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
