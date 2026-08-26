{
  description = "zgui development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    nix-rust-wrangler = {
      url = "github:Janrupf/nix-rust-wrangler";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, nix-rust-wrangler, rust-overlay }:
  let
      # We can re-use this across all nixpkgs instances
      rustOverlayInstance = (import rust-overlay);
  in
  (flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rustOverlayInstance nix-rust-wrangler.overlays.default ];
      };

      nix-rust-wrangler-lib = nix-rust-wrangler.lib.${system};

      # `rust-toolchain.toml` is the single source of truth for the channel and the
      # components. It is read rather than overridden, because `override` replaces the
      # component list instead of extending it and silently drops rustfmt, clippy and
      # miri — which then fail as "the toolchain does not provide the tool".
      toolchainFile = (builtins.fromTOML (builtins.readFile ./rust-toolchain.toml)).toolchain;

      toolchain = pkgs.rust-bin.fromRustupToolchain (toolchainFile // {
        components = (toolchainFile.components or [ ]) ++ [ "rust-analyzer" ];
      });

      toolchainCollection = nix-rust-wrangler-lib.mkToolchainCollection [
        ((nix-rust-wrangler-lib.deriveToolchainInstance toolchain).addName "default")
      ];

      # Loaded with `dlopen` at run time, so they have to be on the library path of the
      # process rather than on the link line.
      runtimeLibraries = with pkgs; [
        libGL
        # Input policy for the console backend. The libraries it needs in turn — libudev,
        # libevdev, mtdev, libwacom, lua — are on its own `RUNPATH`, so this one entry is enough.
        libinput
        libx11
        libxcursor
        libxi
        libxkbcommon
        libxrandr
        # libseat, for the session backend. `seatd` is the package that carries it, and the
        # daemon it also carries is unused here: logind answers on this machine.
        seatd
        vulkan-loader
        wayland
      ];
    in
    {
      devShells.default = pkgs.mkShell {
        NIX_RUST_WRANGLER_TOOLCHAIN_COLLECTION = toolchainCollection;

        nativeBuildInputs = with pkgs; [
          pkg-config
          stdenv.cc
          pkgs.nix-rust-wrangler
          python3
        ];

        buildInputs = with pkgs; [
          # `fontique` resolves system fonts through fontconfig, so the text stack does
          # not compile without it.
          fontconfig
          libdrm
          libxkbcommon
          wayland
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath runtimeLibraries;

        LIBCLANG_PATH = "${pkgs.libclang.lib}/lib";
        BINDGEN_EXTRA_CLANG_ARGS = let
          stdenv = pkgs.stdenv;
        in pkgs.lib.strings.concatStringsSep " " ((map builtins.readFile [
          "${stdenv.cc}/nix-support/libc-crt1-cflags"
          "${stdenv.cc}/nix-support/libc-cflags"
          "${stdenv.cc}/nix-support/cc-cflags"
          "${stdenv.cc}/nix-support/libcxx-cxxflags"
        ]) ++ (
          pkgs.lib.lists.optional
            stdenv.cc.isClang
            "-idirafter ${stdenv.cc.cc}/lib/clang/${pkgs.lib.getVersion stdenv.cc.cc}/include"
        ) ++ (
          pkgs.lib.lists.optional
          stdenv.cc.isGNU
          "-isystem ${stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion stdenv.cc.cc}
           -isystem ${stdenv.cc.cc}/include/c++/${pkgs.lib.getVersion stdenv.cc.cc}/${stdenv.hostPlatform.config}
           -idirafter ${stdenv.cc.cc}/lib/gcc/${stdenv.hostPlatform.config}/${pkgs.lib.getVersion stdenv.cc.cc}/include"
        ));
      };
    }
  ));
}
