{
  description = "AA Editor dev flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url  = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        includeList = [
          ''-I"${pkgs.llvmPackages_latest.libclang.lib}/lib/clang/${pkgs.llvmPackages_latest.libclang.version}/include"''
          ''-I"${pkgs.glib.dev}/include/glib-2.0"''
          ''-I${pkgs.glib.out}/lib/glib-2.0/include/''
        ];
        allCflags = builtins.concatStringsSep " " ++ includeList;
      in
        {
          RUST_NIX_CFLAGS = allCflags;
          devShells.default = with pkgs; mkShell {
            LIBCLANG_PATH = "${pkgs.llvmPackages_latest.libclang.lib}/lib";
            LD_LIBRARY_PATH = lib.makeLibraryPath [
              gtk4
              gdk-pixbuf
              cairo
              fontconfig
              freetype
              libarchive
              gumbo
              jbig2dec
              libpng
              libjpeg
              openjpeg
              leptonica
              tesseract
              zxing
              sqlite
              glib
              zlib
              stdenv.cc.cc.lib
              harfbuzz
              pango
            ];

            buildInputs = [
              openssl
              
              # depend libs
              # for gtk4
              gtk4
              gdk-pixbuf
              cairo
              freetype
              fontconfig

              # for gdk-pixbuf
              libpng
              libjpeg
              openjpeg

              # db
              sqlite

              # c and lib
              glib
              zlib
              # stdenv.cc.cc.lib
            ];
            nativeBuildInputs = with pkgs; [
              # linker
              mold

              pkg-config
              # glibc.dev
              clang

              llvmPackages.bintools

              # rust
              (rust-bin.stable.latest.default.override {
                extensions = [ "rust-src" ];
              })
              rust-analyzer

              gnupg
            ];
            shellHook = ''
            export XDG_DATA_DIRS=$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS
            '';
          };
        }
    );
}
