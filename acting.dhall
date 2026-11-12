{-
life/acting.dhall — every operational script in this repository (`*.sh`,
`*.py`), classified, with a ceiling per class; a test needs no row when its path
says it is one. What the classes mean and when a ceiling may rise:
dev-lint/tables/acting_schema.dhall. The gate runs `dev-lint-fleet --acting .`:
a script with no row, a row for a file that is gone, or a class over its
ceiling fails the commit that causes it.
-}
let A = ../dev-lint/tables/acting_schema.dhall

let subject = A.subject

let Class = A.Class

in  { ceilings =
        -- acts: each row names what it becomes (automation-model items 13-15)
      { acts = 1
      , bootstrap = 0
      , by_design = 0
      , observes = 0
      , observer_checks = 0
      }
    , subjects =
      [ subject
              "android/deploy.sh"
              Class.Acts
              "builds the APK and installs it on a phone over adb: the phone-shells plan's target (automation-model item 14)"
          , subject "docs/design/emotion-space/emocoords.py" Class.OutOfScope ""
          , subject "docs/design/emotion-space/emosvg.py" Class.OutOfScope ""
          , subject "frontend/gen-icons.sh" Class.OutOfScope ""
          , subject "llm-host/src/life_llm_host/__init__.py" Class.OutOfScope ""
          , subject "llm-host/src/life_llm_host/host.py" Class.OutOfScope ""
          , subject "llm-host/src/life_llm_host/model.py" Class.OutOfScope ""
          , subject "scripts/dev-db.sh" Class.OutOfScope ""
          , subject "scripts/gen-types.sh" Class.OutOfScope ""
          , subject
              "scripts/rechecksum-migrations.sh"
              Class.OutOfScope
              "prints SQL for a person to run; runs nothing"
          , subject
              "scripts/seed-demo.sh"
              Class.OutOfScope
              "seeds a local development server"
          , subject "scripts/setup-hooks.sh" Class.OutOfScope ""
          , subject
              "tools/emotion_worker.py"
              Class.OutOfScope
              "life's own worker: posting its results to life's API is the app's job"
      ]
    }
