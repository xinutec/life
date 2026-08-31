{-
life/gate.dhall — this repository's commit gate.

Was `scripts/verify.sh`. One change matters far more than the rest.

**The backend tests are no longer skipped.** The script ran them only when
`LIFE_TEST_DATABASE_URL` was already exported, and printed four lines telling you
how to do that yourself. Its own header explains what that costs: "It used to
omit them silently, which let a rename of a column land green here and fail in
CI — the queries are runtime strings, so running them IS the check on them, and
126 of the 128 query sites are executed by some test here (measured 2026-07-30)."

So the previous repair was to make the skip *loud*. It was still a skip, and 29
of this repository's 47 test files need that variable. Now the row supplies it:
`with-test-db` brings up a throwaway MariaDB, exports the URL, runs the suite and
tears the server down again — the same tool fleetwatch, messages and coach use,
so this costs a row rather than a decision.

Port 3320: fleetwatch's ephemeral server takes 3317, messages' 3318, coach's
3319, and the fleet gate can run all four at once. No `--grant-all`: this suite
uses the one database it is given, and the narrow default is what stops it
inheriting rights it never asked for.

Worth doing next, and deliberately not done here: the tests themselves still
`return` when the variable is unset, so a bare `cargo test` reports green with
none of the SQL exercised. The gate can no longer reach that path, but a
hand-run can. coach's equivalent fails loudly instead, and life's should.

**The `&&` chain is gone.** `pnpm run lint && pnpm run typecheck:e2e && pnpm exec
ng build && pnpm test && pnpm run ui-check` reported one name when five things
could be wrong.

**The build is checked rather than hoped for.** The script set the worker cap and
said a spurious abort "is worked around by re-running verify", so a complete,
valid bundle that hit the macOS Piscina teardown abort failed the gate and cost a
manual re-run, and nothing asserted what the build produced.

**The conditional `pnpm install` is gone**, for the reason every other
conversion's was: its own comment justified it on correctness, and running it
unconditionally serves that better.

The generated `gate.json` is committed; `the table matches its Dhall` re-renders
and diffs it, so running the gate needs no `dhall`.
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
      , {-  The whole suite against a throwaway MariaDB — see the header. This is
            the row the conversion exists for.

            `--test-threads=1` as the script had it: these tests share one
            database and are not written to interleave.
        -}
        G.Check::{
        , name = "tests (against a real MariaDB)"
        , argv =
              G.inDevShell [ "nix", "run", "../dev-lint#with-test-db", "--" ]
            # [ "--database"
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
      , {-  Generated-types drift: regenerate the ts-rs bindings and fail if the
            committed frontend/src/app/generated output moved.
        -}
        G.Check::{
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
      , {-  The L2 phone-width layout harness: serves the dist the build row
            wrote and asserts no overlap or overflow at Pixel width.
        -}
        G.Check::{
        , name = "frontend ui-check (phone-width layout harness)"
        , cwd = "frontend"
        , argv = G.inDevShell [ "pnpm", "run", "ui-check" ]
        , env = G.nonInteractive
        , timeout_s = 1800
        }
      , {-  The Android app is not a bookmark: it holds the clipboard bridge, the
            reminder alarms, and the shop-enrichment WebView — the native half of
            features the web app can only ask for. Toolchain comes from recall's
            android dev shell, the same one android/deploy.sh uses; a missing
            shell FAILS this row rather than skipping it, because a gate that
            skips is a gate that lies. No unit tests on this side yet, so
            `assembleDebug` is the whole check.

            `--no-daemon` is load-bearing. A Gradle daemon outlives the shell
            that started it and is reused by env, so one started outside this
            devshell — with no ANDROID_HOME — serves later builds from inside
            it and they fail with "SDK location not found" while
            `echo $ANDROID_HOME` in the very same shell prints the path. That
            cost a red gate on a commit touching only frontend/public. The
            daemon buys a few seconds; not lying about why a build failed is
            worth more.
        -}
        G.Check::{
        , name = "android :app assembleDebug"
        , cwd = "android"
        , argv =
            [ "nix"
            , "develop"
            , "../../recall#android"
            , "--no-warn-dirty"
            , "--command"
            , "./gradlew"
            , "--console=plain"
            , "--no-daemon"
            , ":app:assembleDebug"
            ]
        , timeout_s = 1800
        }
      , {-  A green gate has to mean the thing home-manager deploys still builds.
            `deploy/hm-agents.nix` runs `${worker}/bin/life-emotion-worker` from a
            STORE PATH built by `nix/emotion-worker.nix`, and none of the rows
            above touch it: `frontend build` is `ng build`, the Rust rows are
            `cargo`, and neither goes near the flake output the agent installs.

            Named after recall's row, which has said this since before it was
            needed anywhere. gamepads and thoth each grew one on 2026-08-15 after
            a pnpm audit refresh moved a lockfile and left the `pnpmDeps` hash
            behind, wedging every home-manager activation on the Mac.

            ⚠ **The cost of this hole is not local.** `~/.config/home-manager/
            switch.sh` re-locks every local input to its committed HEAD in one
            step, so a repository that will not build stops the activation for
            all of them — the failure lands on whatever is being updated, not on
            whoever broke it. life cannot fail the pnpm way (it has no pnpmDeps),
            which is exactly why the row goes in now rather than after it does.
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
