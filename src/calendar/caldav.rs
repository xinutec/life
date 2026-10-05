//! CalDAV: how life writes into Nextcloud, with the Login Flow v2 app password
//! (the login OAuth token cannot reach DAV). A PROPFIND precedes each PUT: a
//! calendar may be renamed or gone, and a read-only subscription looks writable.

use anyhow::{Context, Result, anyhow};
use quick_xml::events::Event as XmlEvent;
use quick_xml::{Reader, XmlVersion};
use reqwest::Method;
use reqwest::header::{CONTENT_TYPE, IF_NONE_MATCH};

use crate::nextcloud::credentials::Credentials;
use crate::nextcloud::login_flow::basic_auth_header;

/// Split only where the caller acts differently: a rejected password needs a
/// re-link.
#[derive(Debug, thiserror::Error)]
pub enum DavError {
    #[error("nextcloud rejected the app password")]
    Unauthorized,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarRef {
    /// As the server spelled it, ending in `/`.
    pub href: String,
    /// So the reply can say where the trip went.
    pub name: String,
}

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

    pub async fn writable_calendar(&self) -> Result<CalendarRef, DavError> {
        let home = calendar_home(self.base, &self.login_name)
            .map_err(|e| DavError::Other(e.context("building the Nextcloud URL")))?;
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

    /// Create-only (`If-None-Match: *`): a plain PUT would overwrite whatever
    /// lived at that path.
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

    fn url(&self, path: &str) -> Result<url::Url, DavError> {
        url::Url::parse(self.base)
            .and_then(|b| b.join(path))
            .map_err(|e| DavError::Other(anyhow!(e).context("building the Nextcloud URL")))
    }
}

fn propfind() -> Method {
    Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method token")
}

/// `<base>/remote.php/dav/calendars/<login>/`, the login percent-encoded. Any path
/// on `base` is replaced: a Nextcloud under a sub-path is unsupported.
pub fn calendar_home(base: &str, login: &str) -> Result<url::Url> {
    let mut url = url::Url::parse(base).context("parsing the Nextcloud base URL")?;
    url.set_path("");
    url.path_segments_mut()
        .map_err(|()| anyhow!("the Nextcloud base URL cannot hold a path: {base}"))?
        .extend(["remote.php", "dav", "calendars", login])
        .push("");
    Ok(url)
}

#[derive(Default)]
struct Collection {
    href: Option<String>,
    name: Option<String>,
    is_calendar: bool,
    is_subscription: bool,
    /// `None` when no set was listed, unlike one listed without `VEVENT`.
    components: Option<Vec<String>>,
    /// `None` when unknown, which is not held against it.
    privileges: Option<Vec<String>>,
}

impl Collection {
    /// Silence permits: the PUT is the real test, and reports its own failure.
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

/// Which calendar a trip goes in, or `None`. The module's whole judgement,
/// testable against a captured server answer.
pub fn writable_from(multistatus: &str) -> Result<Option<CalendarRef>> {
    Ok(choose(calendars(multistatus)?))
}

/// By local element name: prefixes are the server's to choose and have changed;
/// the local names are fixed by RFC 4918 and 4791.
fn calendars(xml: &str) -> Result<Vec<CalendarRef>> {
    let reader = &mut Reader::from_str(xml);

    let mut out = Vec::new();
    let mut current: Option<Collection> = None;
    // Text accumulates until the element closes: "Home &amp; away" arrives in
    // three pieces.
    let mut text_into: Option<&'static str> = None;
    let mut text = String::new();
    let mut inside: Option<&'static str> = None;

    loop {
        match reader
            .read_event()
            .context("reading Nextcloud's calendar list")?
        {
            XmlEvent::Eof => break,
            // A self-closing container (Nextcloud's empty property) emits no End,
            // so it must not open one.
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
            // An unresolvable entity is kept as written: the name is the user's.
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
                                // The first href is the collection's own.
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

/// Each is a child of a property, so its meaning depends on the container.
fn mark(
    element: &quick_xml::events::BytesStart<'_>,
    name: &str,
    inside: Option<&str>,
    collection: Option<&mut Collection>,
) {
    let Some(c) = collection else { return };
    match (inside, name) {
        (Some("resourcetype"), "calendar") => c.is_calendar = true,
        // A subscribed feed carries `<cs:subscribed/>` beside `<cal:calendar/>`.
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
        (Some("privileges"), "privilege") => {}
        (Some("privileges"), granted) => {
            c.privileges
                .get_or_insert_with(Vec::new)
                .push(granted.to_string());
        }
        _ => {}
    }
}

/// `personal` (every account has one), else the first by name. The reply names
/// it, so a wrong pick shows.
fn choose(mut found: Vec<CalendarRef>) -> Option<CalendarRef> {
    found.sort_by_key(|c| c.name.to_lowercase());
    let personal = found.iter().position(|c| slug_of(&c.href) == "personal");
    match personal {
        Some(i) => Some(found.swap_remove(i)),
        None => found.into_iter().next(),
    }
}

fn slug_of(href: &str) -> &str {
    href.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

fn ensure_trailing_slash(mut href: String) -> String {
    if !href.ends_with('/') {
        href.push('/');
    }
    href
}

fn local_name(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw).to_lowercase()
}
