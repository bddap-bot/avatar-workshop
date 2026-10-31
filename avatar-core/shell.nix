let
  rust-overlay = import (fetchTarball {
    url = "https://github.com/oxalica/rust-overlay/archive/6ecf534a7c16e7c969f26838fdd8c93597ae93f3.tar.gz";
    sha256 = "1dyd8r7d0rcknfy7x4dr7vhs49r9z6n327kjz5ibgswwils1bj7s";
  });
  pkgs = import <nixpkgs> { overlays = [ rust-overlay ]; };
  rust = pkgs.rust-bin.stable."1.91.1".default.override {
    targets = [ "aarch64-linux-android" "wasm32-unknown-unknown" ];
  };
in
pkgs.mkShell {
  packages = [ rust pkgs.gcc ];
}
