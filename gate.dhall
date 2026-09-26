{-
life/gate.dhall — this repository's commit gate. `gate.json` is generated from
it and committed; `the table matches its Dhall` re-renders and diffs it.
-}

let G = ../dev-lint/gate/schema.dhall

in  { name = "life"
    , checks =
      [ G.Check::{
        , name = "formatting"
        , argv = G.inDevShell [ "cargo", "fmt", "--all", "--check" ]
        , timeout_s = 180
        }
      , {-  Clippy gets its own target directory: clippy-driver and rustc
            fingerprint the workspace differently and evict each other in a
            shared one, so a plain `cargo build` between commits would force
            clippy to recompile everything. Costs one extra copy of the deps.
        -}
        G.Check::{
        , name = "clippy"
        , argv =
            G.inDevShell
              [ "cargo", "clippy", "--all-targets", "--", "-D", "warnings" ]
        , env = G.clippyTarget
        , timeout_s = 1800
        }
      , G.cargoDoc
      , {-  The whole suite against a throwaway MariaDB: the queries are runtime
            strings, so running them is the only check on them. Port 3320
            because fleetwatch, messages and coach take 3317–3319 and the fleet
            gate runs them together. One thread: the tests share one database.
        -}
        G.Check::{
        , name = "tests (against a real MariaDB)"
        , argv =
            G.withTestDb
              "../"
              [ "--database"
              , "life"
              , "--user"
              , "life"
              , "--password"
              , "life"
              , "--port"
              , "3320"
              , "--url-env"
              , "LIFE_TEST_DATABASE_URL"
              , "--"
              , "cargo"
              , "test"
              , "--"
              , "--test-threads=1"
              ]
        , timeout_s = 3600
        }
      , G.Check::{
        , name = "generated types are current"
        , argv = G.inDevShell [ "scripts/gen-types.sh", "--check" ]
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend deps match the lockfile"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "install", "--frozen-lockfile" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend lint"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "lint" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend formatting"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "format:check" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend typecheck (e2e)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "typecheck:e2e" ]
        , env = G.nonInteractive
        , timeout_s = 900
        }
      , G.Check::{
        , name = "frontend build"
        , cwd = "frontend"
        , argv =
            G.ngBuild
              "../../"
              [ "dist/life-web/browser" ]
              [ "pnpm", "exec", "ng", "build" ]
        , env = G.nonInteractive
        , timeout_s = 1800
        }
      , G.Check::{
        , name = "frontend unit tests"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "test" ]
        , env = G.nonInteractive # G.oneAngularWorker
        , timeout_s = 1800
        }
      , {-  Serves the dist the build row wrote. -}
        G.Check::{
        , name = "frontend ui-check (phone-width layout harness)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "ui-check" ]
        , {-  Playwright deletes this at the start of every run, so the gate copies
              it aside when this check fails.
          -}
          artifacts = [ "test-results" ]
        , env = G.nonInteractive
        , timeout_s = 1800
        }
      , {-  `--no-daemon`: a daemon started outside this shell, without
            ANDROID_HOME, gets reused here and fails "SDK location not found".
        -}
        G.Check::{
        , name = "android :app assembleDebug"
        , cwd = "android"
        , argv =
            [ "nix"
            , "develop"
            , "git+file:../../recall?ref=HEAD#android"
            , "--no-warn-dirty"
            , "--command"
            , "./gradlew"
            , "--console=plain"
            , "--no-daemon"
            , ":app:assembleDebug"
            ]
        , timeout_s = 1800
        }
      , {-  `deploy/hm-agents.nix` runs the worker from this flake output, which
            no other row builds. A repo that fails to build stops home-manager
            activation for every local input, not just this one.
        -}
        G.Check::{
        , name = "the emotion worker builds (what home-manager deploys)"
        , argv =
            [ "nix", "build", "--no-warn-dirty", "--no-link", ".#emotion-worker" ]
        , timeout_s = 1800
        }
      , G.devLint "../"
      , G.checkTable "../dev-lint"
      ]
    }
