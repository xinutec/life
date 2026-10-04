# Dev shells for life. Enter the backend/frontend one with: nix develop
# Pure-Rust TLS (rustls) so there's no openssl/pkg-config native dep.
#
# The Android wrapper (android/) has its own shell — `nix develop .#android` —
# carrying the SDK, JDK and adb.
{
  description = "life — personal home OS backend";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # The LLM host's Python env from llm-host/uv.lock's PyPI wheels: nixpkgs'
    # mlx-lm built from source on aarch64-darwin is uncached.
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    uv2nix = {
      url = "github:pyproject-nix/uv2nix";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    pyproject-build-systems = {
      url = "github:pyproject-nix/build-system-pkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
      inputs.uv2nix.follows = "uv2nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, pyproject-nix, uv2nix, pyproject-build-systems }:
    let
      systems = [ "aarch64-darwin" "x86_64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});

      # The emotion worker's model host (llm-host/), Apple Silicon only: MLX runs
      # on Metal, and the lock resolves for darwin alone.
      llmHost = pkgs:
        let
          workspace = uv2nix.lib.workspace.loadWorkspace { workspaceRoot = ./llm-host; };
          # mlx ships as two wheels that expect one directory: `mlx` has the .so,
          # `mlx-metal` has libmlx.dylib, found through @rpath. nix gives each wheel
          # its own store path, so link mlx-metal's lib/ in where the loader looks.
          mlxMetalFix = final: prev: {
            mlx = prev.mlx.overrideAttrs (old: {
              postInstall = (old.postInstall or "") + ''
                for sp in $out/lib/python*/site-packages/mlx; do
                  mkdir -p "$sp/lib"
                  ln -sfn ${final."mlx-metal"}/lib/python*/site-packages/mlx/lib/* "$sp/lib/"
                done
              '';
            });
          };
          pythonSet =
            (pkgs.callPackage pyproject-nix.build.packages { python = pkgs.python312; })
            .overrideScope (nixpkgs.lib.composeManyExtensions [
              pyproject-build-systems.overlays.default
              (workspace.mkPyprojectOverlay { sourcePreference = "wheel"; })
              mlxMetalFix
            ]);
        in
        {
          # What the launchd agent runs (deploy/hm-agents.nix).
          llm-host = pythonSet.mkVirtualEnv "life-llm-host" workspace.deps.default;
          # The same plus pytest: what the gate tests.
          llm-host-dev = pythonSet.mkVirtualEnv "life-llm-host-dev" workspace.deps.all;
        };
      darwinOnly = pkgs: nixpkgs.lib.optionalAttrs (pkgs.stdenv.hostPlatform.system == "aarch64-darwin");

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
            platformToolsVersion = "37.0.1"; # adb
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
            # stderr: on stdout it would prefix `nix develop -c <cmd>` output.
            echo "life android devshell — adb + sdk: $ANDROID_HOME" >&2
          '';
        };
    in {
      # The emotion-suggestion worker, packaged so launchd runs a store path
      # rather than the working tree (deploy/hm-agents.nix). Run it by hand with
      # `nix run .#emotion-worker`.
      packages = forAll (pkgs: {
        emotion-worker = pkgs.callPackage ./nix/emotion-worker.nix { };
      } // darwinOnly pkgs (llmHost pkgs));

      devShells = nixpkgs.lib.genAttrs systems (system: {
        default = nixpkgs.legacyPackages.${system}.mkShell {
          # Playwright's browsers come from the lock, not ~/Library/Caches: the
          # driver's version must match @playwright/test's (tables/deps.dhall).
          PLAYWRIGHT_BROWSERS_PATH = nixpkgs.legacyPackages.${system}.playwright-driver.browsers;
          PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "1";
          packages = with nixpkgs.legacyPackages.${system}; [
            cargo
            rustc
            rust-analyzer
            rustfmt
            clippy
            sqlx-cli
            nodejs_24 # Angular 22 frontend (frontend/)
            pnpm # the frontend's installer; node ships npm too, ignore it
          ];
        };

        # Build: nix develop .#android --command ./gradlew -p android assembleDebug
        # Debug the WebView on a device: nix develop .#android --command adb logcat
        android = androidShell system;
      } // darwinOnly nixpkgs.legacyPackages.${system} {
        # The LLM host's tests: nix develop .#llm-host --command pytest llm-host/tests
        llm-host = nixpkgs.legacyPackages.${system}.mkShell {
          packages = [ self.packages.${system}.llm-host-dev ];
        };
      });
    };
}
