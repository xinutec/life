//! CalDAV — the one way life writes into Nextcloud.
//!
//! "life never writes to NC's database" is read as *no schema surgery*: CalDAV
//! is the supported client protocol, the same one DAVx⁵ and every phone speak,
//! so a `PUT` here is a normal calendar client doing a normal thing
//! (docs/design/overview.md §2b).
//!
//! Authentication is HTTP Basic with the app password from Login Flow v2. The
//! identity OAuth2 token cannot reach these endpoints at all, which is why
//! there are two Nextcloud credentials in this app rather than one.
//!
//! ## Why there is a PROPFIND before the PUT
//!
//! The calendar *home* is a known path — `/remote.php/dav/calendars/<user>/` —
//! but the collection inside it is not. `personal` is what Nextcloud creates,
//! and it is also the first thing people rename, delete, or bury among five
//! others. Worse, a **subscription** lives in that same home and looks like a
//! calendar until you write to it: the bins feed this app reads is exactly the
//! kind of thing that is subscribed there, and a shop trip `PUT` into a
//! read-only mirror of the council's calendar is the failure this asks the
//! server about rather than assumes away.

use anyhow::{Context, Result, anyhow};
use quick_xml::events::Event as XmlEvent;
use quick_xml::{Reader, XmlVersion};
use reqwest::Method;
use reqwest::header::{CONTENT_TYPE, IF_NONE_MATCH};

use crate::nextcloud::credentials::Credentials;
use crate::nextcloud::login_flow::basic_auth_header;

/// A failure talking to Nextcloud, split only where the caller must act
/// differently: a rejected password is the user's to fix by re-linking, and
/// everything else is ours or the server's.
#[derive(Debug, thiserror::Error)]
pub enum DavError {
    #[error("nextcloud rejected the app password")]
    Unauthorized,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// One calendar collection on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarRef {
    /// Server path, as the server spelled it — always ending in `/`.
    pub href: String,
    /// What the user calls it. Worth carrying because we tell them where the
    /// trip went: "added to Personal" is checkable, "added" is not.
    pub name: String,
}

/// An authenticated CalDAV conversation with one account.
pub struct Dav<'a> {
    http: &'a reqwest::Client,
    base: &'a str,
    login_name: String,
    auth: String,
}

const PROPFIND_CALENDARS: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:cal="urn:ietf:params:xml:ns:caldav">
  <d:prop>
    <d:resourcetype/>
    <d:displayname/>
    <cal:supported-calendar-component-set/>
    <d:current-user-privileges/>
  </d:prop>
</d:propfind>"#;

impl<'a> Dav<'a> {
    pub fn new(http: &'a reqwest::Client, base: &'a str, creds: &Credentials) -> Self {
        Self {
            http,
            base,
            login_name: creds.login_name.clone(),
            auth: basic_auth_header(&creds.login_name, &creds.app_password),
        }
    }

    /// The calendar to write a shop trip to.
    pub async fn writable_calendar(&self) -> Result<CalendarRef, DavError> {
        let home = self.url(&format!(
            "/remote.php/dav/calendars/{}/",
            urlencoding(&self.login_name)
        ))?;
        let res = self
            .http
            .request(propfind(), home)
            .header("Depth", "1")
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .header(reqwest::header::AUTHORIZATION, &self.auth)
            .body(PROPFIND_CALENDARS)
            .send()
            .await
            .map_err(|e| DavError::Other(anyhow!(e).context("listing your calendars")))?;
        let status = res.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(DavError::Unauthorized);
        }
        if status != reqwest::StatusCode::MULTI_STATUS {
            return Err(DavError::Other(anyhow!(
                "listing your calendars: Nextcloud answered HTTP {status}"
            )));
        }
        let body = res
            .text()
            .await
            .map_err(|e| DavError::Other(anyhow!(e).context("reading the calendar list")))?;
        writable_from(&body)
            .map_err(DavError::Other)?
            .ok_or_else(|| {
                DavError::Other(anyhow!(
                    "no calendar on this Nextcloud account accepts events — \
                     every collection is a subscription or read-only"
                ))
            })
    }

    /// Create one event. Returns the path it was created at.
    ///
    /// `If-None-Match: *` makes this create-only. The UID is ours and freshly
    /// minted, so a collision means something is wrong with that assumption —
    /// and the alternative, a plain `PUT`, would quietly overwrite whatever
    /// event already lived at that path.
    pub async fn put_event(
        &self,
        calendar: &CalendarRef,
        uid: &str,
        ics: &str,
    ) -> Result<String, DavError> {
        let path = format!("{}{uid}.ics", calendar.href);
        let url = self.url(&path)?;
        let res = self
            .http
            .put(url)
            .header(CONTENT_TYPE, "text/calendar; charset=utf-8")
            .header(IF_NONE_MATCH, "*")
            .header(reqwest::header::AUTHORIZATION, &self.auth)
            .body(ics.to_string())
            .send()
            .await
            .map_err(|e| DavError::Other(anyhow!(e).context("saving the event")))?;
        match res.status() {
            s if s.is_success() => Ok(path),
            reqwest::StatusCode::UNAUTHORIZED => Err(DavError::Unauthorized),
            reqwest::StatusCode::FORBIDDEN => Err(DavError::Other(anyhow!(
                "Nextcloud would not add an event to “{}”",
                calendar.name
            ))),
            reqwest::StatusCode::PRECONDITION_FAILED => Err(DavError::Other(anyhow!(
                "an event already exists at {path}"
            ))),
            s => Err(DavError::Other(anyhow!(
                "saving the event: Nextcloud answered HTTP {s}"
            ))),
        }
    }

    /// Resolve a server path against the configured Nextcloud origin.
    fn url(&self, path: &str) -> Result<url::Url, DavError> {
        url::Url::parse(self.base)
            .and_then(|b| b.join(path))
            .map_err(|e| DavError::Other(anyhow!(e).context("building the Nextcloud URL")))
    }
}

fn propfind() -> Method {
    Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method token")
}

/// Percent-encode a path segment. Login names are usually plain, but they may
/// legitimately contain a space or an `@` (an email-style login), and an
/// unescaped one makes a URL that resolves somewhere else or nowhere.
fn urlencoding(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// What one `<response>` in the multistatus said about itself.
#[derive(Default)]
struct Collection {
    href: Option<String>,
    name: Option<String>,
    is_calendar: bool,
    is_subscription: bool,
    /// `None` when the server listed no component set at all, which is not the
    /// same as listing one without `VEVENT`.
    components: Option<Vec<String>>,
    /// `None` when no privilege set came back — unknown, so not held against it.
    privileges: Option<Vec<String>>,
}

impl Collection {
    /// Whether a shop trip may be written here.
    ///
    /// Every test is phrased so that *silence permits*: a server that declines
    /// to answer one of these questions should not lose you the calendar it
    /// otherwise agreed is a writable calendar. The `PUT` is the real test, and
    /// it reports its own failure clearly.
    fn accepts_events(&self) -> bool {
        if !self.is_calendar || self.is_subscription {
            return false;
        }
        let takes_vevents = self
            .components
            .as_ref()
            .is_none_or(|c| c.iter().any(|c| c.eq_ignore_ascii_case("VEVENT")));
        let writable = self.privileges.as_ref().is_none_or(|p| {
            p.iter()
                .any(|p| matches!(p.as_str(), "write" | "write-content" | "all"))
        });
        takes_vevents && writable
    }
}

/// Which calendar in the server's `D:multistatus` a shop trip goes in, or
/// `None` if the account has nowhere to put one.
///
/// The whole judgement of this module — what counts as a writable calendar, and
/// which one to prefer — reachable from a string, so it can be tested against a
/// real server's answer without a server.
pub fn writable_from(multistatus: &str) -> Result<Option<CalendarRef>> {
    Ok(choose(calendars(multistatus)?))
}

/// Every collection in a `D:multistatus` that will take a `VEVENT`.
///
/// Read by local element name throughout: the namespace prefixes (`d:`, `DAV:`,
/// `cal:`, `x1:`) are the server's to choose and Nextcloud has changed them
/// before, while the local names are fixed by RFC 4918 and RFC 4791.
fn calendars(xml: &str) -> Result<Vec<CalendarRef>> {
    let reader = &mut Reader::from_str(xml);

    let mut out = Vec::new();
    let mut current: Option<Collection> = None;
    // Which element's text we are collecting, and which list-valued property we
    // are inside. Both are shallow — this document has no recursion in it.
    //
    // Text is ACCUMULATED rather than taken per event, and trimmed only when the
    // element closes: an entity arrives as its own event, so "Home &amp; away"
    // reaches us as three pieces, and a reader that took the last one would call
    // that calendar "away".
    let mut text_into: Option<&'static str> = None;
    let mut text = String::new();
    let mut inside: Option<&'static str> = None;

    loop {
        match reader
            .read_event()
            .context("reading Nextcloud's calendar list")?
        {
            XmlEvent::Eof => break,
            // Only a Start opens a container. A self-closing one — which is how
            // Nextcloud spells a property it has no value for, in the 404
            // `<propstat>` — closes immediately and emits no End, so treating it
            // as an opening would leave `inside` stuck and read every element
            // after it as that container's contents.
            XmlEvent::Start(e) => {
                let name = local_name(e.local_name().as_ref());
                match name.as_str() {
                    "response" => current = Some(Collection::default()),
                    "href" => {
                        text_into = Some("href");
                        text.clear();
                    }
                    "displayname" => {
                        text_into = Some("displayname");
                        text.clear();
                    }
                    "resourcetype" => inside = Some("resourcetype"),
                    "supported-calendar-component-set" => inside = Some("components"),
                    "current-user-privileges" => inside = Some("privileges"),
                    _ => mark(&e, &name, inside, current.as_mut()),
                }
            }
            XmlEvent::Empty(e) => {
                let name = local_name(e.local_name().as_ref());
                mark(&e, &name, inside, current.as_mut());
            }
            XmlEvent::Text(t) if text_into.is_some() => {
                text.push_str(
                    &t.xml_content(XmlVersion::Implicit1_0)
                        .context("decoding a calendar property")?,
                );
            }
            // `&amp;` and friends arrive as their own event. An unresolvable one
            // is passed through as written rather than dropped — a name is the
            // user's, and mangling it is worse than showing the escape.
            XmlEvent::GeneralRef(r) if text_into.is_some() => {
                let name = r.decode().context("decoding a calendar property")?;
                match quick_xml::escape::resolve_predefined_entity(&name) {
                    Some(resolved) => text.push_str(resolved),
                    None => {
                        text.push('&');
                        text.push_str(&name);
                        text.push(';');
                    }
                }
            }
            XmlEvent::End(e) => {
                let name = local_name(e.local_name().as_ref());
                match name.as_str() {
                    "href" | "displayname" => {
                        let field = text_into.take();
                        let value = text.trim().to_string();
                        text.clear();
                        if let (Some(c), false) = (current.as_mut(), value.is_empty()) {
                            match field {
                                // The first href in a response is the
                                // collection's own; later ones (if a server
                                // volunteers any) are not.
                                Some("href") if c.href.is_none() => c.href = Some(value),
                                Some("displayname") => c.name = Some(value),
                                _ => {}
                            }
                        }
                    }
                    "resourcetype"
                    | "supported-calendar-component-set"
                    | "current-user-privileges" => inside = None,
                    "response" => {
                        if let Some(c) = current.take()
                            && c.accepts_events()
                            && let Some(href) = c.href
                        {
                            let name = c.name.unwrap_or_else(|| slug_of(&href).to_string());
                            out.push(CalendarRef {
                                href: ensure_trailing_slash(href),
                                name,
                            });
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Record what one element inside a container says about the collection.
///
/// Each of these is a child of a property rather than the property itself —
/// `<cal:calendar/>` inside `resourcetype`, `<cal:comp name="VEVENT"/>` inside
/// the component set, `<d:write-content/>` inside a privilege — so what it means
/// depends entirely on which container we are in.
fn mark(
    element: &quick_xml::events::BytesStart<'_>,
    name: &str,
    inside: Option<&str>,
    collection: Option<&mut Collection>,
) {
    let Some(c) = collection else { return };
    match (inside, name) {
        (Some("resourcetype"), "calendar") => c.is_calendar = true,
        // Nextcloud marks a subscribed feed with `<cs:subscribed/>` *alongside*
        // `<cal:calendar/>`, so this has to be looked for even though the
        // collection has already said it is a calendar.
        (Some("resourcetype"), "subscribed") => c.is_subscription = true,
        (Some("components"), "comp") => {
            let value = element
                .try_get_attribute("name")
                .ok()
                .flatten()
                .and_then(|a| {
                    a.normalized_value(XmlVersion::Implicit1_0)
                        .ok()
                        .map(|v| v.into_owned())
                });
            if let Some(value) = value {
                c.components.get_or_insert_with(Vec::new).push(value);
            }
        }
        // `<d:privilege><d:write-content/></d:privilege>` — the privilege is the
        // child, so the wrapper itself carries no information.
        (Some("privileges"), "privilege") => {}
        (Some("privileges"), granted) => {
            c.privileges
                .get_or_insert_with(Vec::new)
                .push(granted.to_string());
        }
        _ => {}
    }
}

/// Which calendar a trip goes in when the account has several.
///
/// `personal` first — the collection Nextcloud makes for every account, and the
/// one a person with one calendar means without being asked. Otherwise the
/// first by name, which is arbitrary but *stable*: the same account picks the
/// same calendar every time, so the answer we report is worth reading once.
///
/// Choosing rather than asking is a deliberate v1: the reply names the calendar
/// it used, which turns a wrong guess into something you can see and say so
/// about, rather than a setting you have to find before the feature works.
fn choose(mut found: Vec<CalendarRef>) -> Option<CalendarRef> {
    found.sort_by_key(|c| c.name.to_lowercase());
    let personal = found.iter().position(|c| slug_of(&c.href) == "personal");
    match personal {
        Some(i) => Some(found.swap_remove(i)),
        None => found.into_iter().next(),
    }
}

/// The last non-empty path segment — a collection's own name in the URL.
fn slug_of(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

fn ensure_trailing_slash(mut href: String) -> String {
    if !href.ends_with('/') {
        href.push('/');
    }
    href
}

/// Element names arrive as bytes and are ASCII by the XML spec's own rules for
/// these vocabularies; anything else is not a name we match on anyway.
fn local_name(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).to_lowercase()
}
