//! What the DB integration tests share: the database they need, or a failure.
//!
//! These tests are the only check on this repository's SQL. The queries are
//! runtime strings, so running them IS the check on them, and 126 of the 128
//! query sites are executed by some test here (measured 2026-07-30).
//!
//! They used to return early when `LIFE_TEST_DATABASE_URL` was unset, after an
//! `eprintln!` nobody sees — `cargo test` captures stderr from passing tests. So
//! a bare `cargo test` reported `ok. 6 passed` in 0.00s with none of the SQL
//! exercised, and looked exactly like a run that had done the work. 57 test
//! functions across 29 files did this.
//!
//! The gate cannot reach that path any more — its `tests` row brings up a
//! throwaway MariaDB and exports the variable — but a hand-run could, and a
//! hand-run is what you do while you are changing a query. So this panics
//! instead, which is coach's answer to the same question: a test that silently
//! passes when it cannot run is worse than no test, because it reports the
//! coverage it is not providing.

/// The test database, or a failure that says how to get one.
pub(crate) fn test_db_url() -> String {
    std::env::var("LIFE_TEST_DATABASE_URL").unwrap_or_else(|_| {
        panic!(
            "LIFE_TEST_DATABASE_URL is unset, and these tests are the only check \
             on the SQL — so this is a failure rather than a skip.\n\
             \n\
             Run the whole gate, which supplies a throwaway MariaDB itself:\n\
             \x20   nix run ../dev-lint#gate -- . gate.json\n\
             \n\
             Or just this suite against one:\n\
             \x20   nix develop --command nix run ../dev-lint#with-test-db -- \\\n\
             \x20     --database life --user life --password life --port 3320 \\\n\
             \x20     --url-env LIFE_TEST_DATABASE_URL -- cargo test -- --test-threads=1"
        )
    })
}
