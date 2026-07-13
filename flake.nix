# Dev shells for life. Enter the backend/frontend one with: nix develop
# Pure-Rust TLS (rustls) so there's no openssl/pkg-config native dep.
#
# The Android wrapper (android/) has its own shell — `nix develop .#android` —
# carrying the SDK, JDK and adb. It lives here, next to the app it builds: having
# to borrow another repo's shell to debug life's own APK is how you end up
# concluding "adb isn't installed" and abandoning a live lead.
{
  description = "life — personal home OS backend";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});

      # The Android SDK is unfree, so it gets its own pkgs import — keeping that
      # licence exception scoped to the shell that needs it. Versions track
      # android/app/build.gradle.kts (compileSdk 36, buildTools 36.0.0, JDK 17).
      androidShell = system:
        let
          pkgs = import nixpkgs {
            inherit system;
            config.allowUnfree = true;
            config.android_sdk.accept_license = true;
          };
          sdk = (pkgs.androidenv.composeAndroidPackages {
            cmdLineToolsVersion = "13.0";
            platformToolsVersion = "35.0.2"; # adb
            buildToolsVersions = [ "36.0.0" ];
            platformVersions = [ "36" ];
            abiVersions = [ ];
            includeNDK = false;
            includeSystemImages = false;
            includeEmulator = false;
          }).androidsdk;
          home = "${sdk}/libexec/android-sdk";
        in
        pkgs.mkShell {
          packages = [ pkgs.jdk17 sdk pkgs.ktlint ];
          shellHook = ''
            export ANDROID_HOME="${home}"
            export ANDROID_SDK_ROOT="${home}"
            export JAVA_HOME="${pkgs.jdk17.home}"
            echo "life android devshell — adb + sdk: $ANDROID_HOME"
          '';
        };
    in {
      devShells = nixpkgs.lib.genAttrs systems (system: {
        default = nixpkgs.legacyPackages.${system}.mkShell {
          packages = with nixpkgs.legacyPackages.${system}; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
            sqlx-cli
            nodejs_24 # Angular 22 frontend (frontend/)
          ];
        };

        # Build: nix develop .#android --command ./gradlew -p android assembleDebug
        # Debug the WebView on a device: nix develop .#android --command adb logcat
        android = androidShell system;
      });
    };
}
