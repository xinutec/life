//! What the DB integration tests share. The queries are runtime strings, so
//! these tests are the only check on the SQL: a missing database fails rather
//! than skips.

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
