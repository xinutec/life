//! Picking the calendar a shop trip goes in, from what the server says.
//!
//! The fixtures are shaped like Nextcloud's own `PROPFIND` answers — namespace
//! prefixes, `<propstat>` blocks, 404s for props a collection doesn't have —
//! because every one of those is something a reader written against a tidier
//! imagined XML would get wrong.
//!
//! The case that matters most: this app *reads a subscribed bin calendar*. If
//! the household subscribes to that feed in Nextcloud, it appears in the same
//! list as the real calendars and answers "yes, I am a calendar". Writing a
//! shop trip into it would be writing to a read-only mirror of the council's
//! data.

use life::calendar::caldav::writable_from;

/// One `<response>` with the props Nextcloud returns for a real calendar.
fn calendar(href: &str, name: &str) -> String {
    format!(
        r#"  <d:response>
    <d:href>{href}</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>{name}</d:displayname>
        <cal:supported-calendar-component-set>
          <cal:comp name="VEVENT"/>
          <cal:comp name="VTODO"/>
        </cal:supported-calendar-component-set>
        <d:current-user-privileges>
          <d:privilege><d:write-content/></d:privilege>
          <d:privilege><d:read/></d:privilege>
        </d:current-user-privileges>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
"#
    )
}

fn multistatus(responses: &[String]) -> String {
    format!(
        r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/">
{}</d:multistatus>"#,
        responses.concat()
    )
}

#[test]
fn the_only_calendar_is_the_one_it_picks() {
    let xml = multistatus(&[calendar(
        "/remote.php/dav/calendars/sam/personal/",
        "Personal",
    )]);
    let picked = writable_from(&xml).unwrap().expect("a calendar");
    assert_eq!(picked.href, "/remote.php/dav/calendars/sam/personal/");
    assert_eq!(picked.name, "Personal");
}

#[test]
fn a_subscribed_feed_is_not_somewhere_to_write() {
    // Nextcloud marks a subscription with `<cs:subscribed/>` *alongside*
    // `<cal:calendar/>` — it does still claim to be a calendar, so a reader that
    // stopped at resourcetype/calendar would happily PUT into it.
    let bins = r#"  <d:response>
    <d:href>/remote.php/dav/calendars/sam/bin-collections/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/><cs:subscribed/></d:resourcetype>
        <d:displayname>Bin collections</d:displayname>
        <cal:supported-calendar-component-set><cal:comp name="VEVENT"/></cal:supported-calendar-component-set>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
"#;
    // Alphabetically "Bin collections" sorts first, so a picker that merely
    // sorted would land on it.
    let xml = multistatus(&[
        bins.to_string(),
        calendar("/remote.php/dav/calendars/sam/work/", "Work"),
    ]);
    let picked = writable_from(&xml).unwrap().expect("a calendar");
    assert_eq!(picked.name, "Work");
}

#[test]
fn a_read_only_share_is_not_somewhere_to_write() {
    let shared = r#"  <d:response>
    <d:href>/remote.php/dav/calendars/sam/alexs-calendar_shared_by_alex/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>A shared calendar</d:displayname>
        <cal:supported-calendar-component-set><cal:comp name="VEVENT"/></cal:supported-calendar-component-set>
        <d:current-user-privileges>
          <d:privilege><d:read/></d:privilege>
        </d:current-user-privileges>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
"#;
    let xml = multistatus(&[
        shared.to_string(),
        calendar("/remote.php/dav/calendars/sam/work/", "Work"),
    ]);
    assert_eq!(writable_from(&xml).unwrap().unwrap().name, "Work");
}

#[test]
fn a_collection_that_takes_no_events_is_skipped() {
    let tasks = r#"  <d:response>
    <d:href>/remote.php/dav/calendars/sam/tasks/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>Aa Tasks</d:displayname>
        <cal:supported-calendar-component-set><cal:comp name="VTODO"/></cal:supported-calendar-component-set>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
"#;
    let xml = multistatus(&[
        tasks.to_string(),
        calendar("/remote.php/dav/calendars/sam/work/", "Work"),
    ]);
    assert_eq!(writable_from(&xml).unwrap().unwrap().name, "Work");
}

#[test]
fn the_home_itself_is_not_a_calendar() {
    // Depth 1 lists the collection you asked about first. It is a plain
    // collection, not a calendar, and must not be written to.
    let home = r#"  <d:response>
    <d:href>/remote.php/dav/calendars/sam/</d:href>
    <d:propstat>
      <d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
"#;
    let xml = multistatus(&[
        home.to_string(),
        calendar("/remote.php/dav/calendars/sam/personal/", "Personal"),
    ]);
    assert_eq!(writable_from(&xml).unwrap().unwrap().name, "Personal");
}

#[test]
fn personal_wins_over_a_name_that_sorts_earlier() {
    // The one Nextcloud makes for every account is what a person with one
    // calendar means, whatever else is in the list.
    let xml = multistatus(&[
        calendar("/remote.php/dav/calendars/sam/allotment/", "Allotment"),
        calendar("/remote.php/dav/calendars/sam/personal/", "Personal"),
    ]);
    assert_eq!(writable_from(&xml).unwrap().unwrap().name, "Personal");
}

#[test]
fn without_a_personal_calendar_the_choice_is_stable() {
    // Arbitrary is fine; changing between trips is not — the reply names the
    // calendar, and that is only worth reading if it stays true.
    let forwards = multistatus(&[
        calendar("/remote.php/dav/calendars/sam/work/", "Work"),
        calendar("/remote.php/dav/calendars/sam/allotment/", "Allotment"),
    ]);
    let backwards = multistatus(&[
        calendar("/remote.php/dav/calendars/sam/allotment/", "Allotment"),
        calendar("/remote.php/dav/calendars/sam/work/", "Work"),
    ]);
    assert_eq!(
        writable_from(&forwards).unwrap().unwrap(),
        writable_from(&backwards).unwrap().unwrap()
    );
}

#[test]
fn a_server_that_answers_less_is_not_held_against_it() {
    // No component set and no privilege list — older servers, and Nextcloud's
    // own 404 propstats, both look like this. Silence is not a refusal; the PUT
    // is the real test and it reports its own failure.
    let sparse = r#"  <d:response>
    <d:href>/remote.php/dav/calendars/sam/personal/</d:href>
    <d:propstat>
      <d:prop>
        <d:resourcetype><d:collection/><cal:calendar/></d:resourcetype>
        <d:displayname>Personal</d:displayname>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
    <d:propstat>
      <d:prop><cal:supported-calendar-component-set/><d:current-user-privileges/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
  </d:response>
"#;
    let picked = writable_from(&multistatus(&[sparse.to_string()]))
        .unwrap()
        .expect("a calendar");
    assert_eq!(picked.name, "Personal");
}

#[test]
fn an_ampersand_in_the_name_comes_back_as_an_ampersand() {
    let xml = multistatus(&[calendar(
        "/remote.php/dav/calendars/sam/home/",
        "Home &amp; away",
    )]);
    assert_eq!(writable_from(&xml).unwrap().unwrap().name, "Home & away");
}

#[test]
fn nowhere_to_write_is_none_rather_than_a_guess() {
    let xml = multistatus(&[]);
    assert!(writable_from(&xml).unwrap().is_none());
}

// ── the calendar home URL ───────────────────────────────────────────────────
//
// Built segment by segment so `url` does the percent-encoding. A login is not
// always the tidy word the happy path assumes: Nextcloud accepts email-style
// logins, and a login carrying a `/` must land as one escaped segment rather
// than opening a new one — that last case is the reason this is not a
// `format!`.

use life::calendar::caldav::calendar_home;

#[test]
fn a_plain_login_gets_the_ordinary_dav_path() {
    let url = calendar_home("https://cloud.example.org", "pippijn").unwrap();
    assert_eq!(
        url.as_str(),
        "https://cloud.example.org/remote.php/dav/calendars/pippijn/"
    );
}

#[test]
fn an_email_style_login_is_escaped_rather_than_sent_raw() {
    let url = calendar_home("https://cloud.example.org", "a b@example.org").unwrap();
    assert_eq!(
        url.as_str(),
        "https://cloud.example.org/remote.php/dav/calendars/a%20b@example.org/"
    );
}

#[test]
fn a_slash_in_a_login_cannot_open_a_new_path_segment() {
    let url = calendar_home("https://cloud.example.org", "../../evil").unwrap();
    // One segment, escaped — not a walk up the tree.
    assert_eq!(
        url.as_str(),
        "https://cloud.example.org/remote.php/dav/calendars/..%2F..%2Fevil/"
    );
}

#[test]
fn a_path_on_the_base_is_replaced_not_appended() {
    // Documents today's behaviour, which the `join("/remote.php/…")` this grew
    // out of also had. A Nextcloud under a sub-path would want appending — a
    // real question, and deliberately not answered here.
    let url = calendar_home("https://cloud.example.org/nextcloud", "pippijn").unwrap();
    assert_eq!(
        url.as_str(),
        "https://cloud.example.org/remote.php/dav/calendars/pippijn/"
    );
}
