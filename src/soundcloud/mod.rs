mod error;
mod format;
mod track;
mod user;
mod util;

use self::util::http::retry_execute;
use lazy_static::lazy_static;
use log::*;
use rayon::prelude::*;
use regex::bytes::Regex;
use reqwest::blocking::{self, RequestBuilder};
use reqwest::{header, Method, Url};
use serde::de::DeserializeOwned;
use std::fmt;
use std::str;
use url;

pub use self::error::Error;
pub use self::track::Track;
pub use self::user::User;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:71.0) Gecko/20100101 Firefox/71.0";
const PAGE_MAX_SIZE: u64 = 200;

pub(crate) fn default_headers() -> header::HeaderMap {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static(USER_AGENT),
    );
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static("https://soundcloud.com/"),
    );
    headers.insert(
        header::ORIGIN,
        header::HeaderValue::from_static("https://soundcloud.com/"),
    );
    headers
}

pub(crate) fn default_client() -> &'static blocking::Client {
    lazy_static! {
        static ref DEFAULT_CLIENT: blocking::Client = blocking::Client::builder()
            .default_headers(default_headers())
            .build()
            .unwrap();
    }
    &DEFAULT_CLIENT
}

#[derive(Clone)]
pub struct Client {
    client: blocking::Client,
    client_id: String,
    token: Option<String>,
}

impl Client {
    /// Set up a client by logging in using the online form, just like a user would in the web
    /// application.
    ///
    /// This login method is not guaranteed to be stable!
    pub fn login(username: impl AsRef<str>, password: impl AsRef<str>) -> Result<Client, Error> {
        let client = default_client();
        let client_id = anonymous_client_id(&client)?;

        let token = {
            trace!("performing password login with user: {}", username.as_ref());
            let login_req_body = PasswordLoginReqBody {
                client_id: &client_id,
                scope: "fast-connect non-expiring purchase signup upload",
                recaptcha_pubkey: "6LeAxT8UAAAAAOLTfaWhndPCjGOnB54U1GEACb7N",
                recaptcha_response: None,
                credentials: Credentials {
                    identifier: username.as_ref(),
                    password: password.as_ref(),
                },
                signature: "8:3-1-28405-134-1638720-1024-0-0:4ab691:2",
                device_id: "381629-667600-267798-887023",
                user_agent: USER_AGENT,
            };
            let login_url = Url::parse_with_params(
                "https://api-v2.soundcloud.com/sign-in/password?app_version=1541509103&app_locale=en",
                &[("client_id", &client_id)],
            ).unwrap();
            trace!("password login URL: {}", login_url);
            let login_res_body: PasswordLoginResBody = retry_execute(
                client,
                client.post(login_url).json(&login_req_body).build()?,
            )?
            .error_for_status()?
            .json()?;
            login_res_body.session.access_token
        };

        trace!("SoundCloud login got token: {}****", &token[0..4]);
        Client::from_token(client_id, token)
    }

    // Attempt to create a client with read-only access to the public API.
    pub fn anonymous() -> Result<Client, Error> {
        let client = default_client();
        let client_id = anonymous_client_id(&client)?;
        Ok(Client {
            client: client.clone(),
            client_id,
            token: None,
        })
    }

    fn from_token(client_id: impl Into<String>, token: impl Into<String>) -> Result<Client, Error> {
        let token = token.into();
        let auth_client = blocking::Client::builder()
            .default_headers({
                let auth_header = format!("OAuth {}", token).parse()?;
                let mut headers = default_headers();
                headers.insert(header::AUTHORIZATION, auth_header);
                headers
            })
            .build()?;
        Ok(Client {
            client: auth_client,
            client_id: client_id.into(),
            token: Some(token),
        })
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<(RequestBuilder, Url), Error> {
        let url = Url::parse_with_params(base_url.as_ref(), &[("client_id", &self.client_id)])?;
        let req = self.client.request(method, url.clone());
        Ok((req, url))
    }

    pub(crate) fn query_string(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<String, Error> {
        let (req, url) = self.request(method.clone(), base_url)?;
        info!("querying {} {}", method, url);
        let s = retry_execute(&self.client, req.build()?)?
            .error_for_status()?
            .text()?;
        Ok(s)
    }

    pub(crate) fn query<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<T, Error> {
        let (req, url) = self.request(method.clone(), base_url)?;
        info!("querying {} {}", method, url);
        let mut buf = Vec::new();
        retry_execute(&self.client, req.build()?)?
            .error_for_status()?
            .copy_to(&mut buf)?;

        match serde_json::from_slice(&buf[..]) {
            Ok(t) => Ok(t),
            Err(err) => {
                let body = String::from_utf8_lossy(&buf[..]);
                warn!("bad body: {}", body);
                warn!("bad body error: {}", err);
                Err(Error::MalformedResponse {
                    method,
                    url,
                    body: body.to_string(),
                    error: Box::new(err),
                })
            }
        }
    }
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let token = self
            .token
            .as_ref()
            .filter(|t| t.len() >= 4)
            .map(|t| format!("{}****", &t[0..4]))
            .unwrap_or_else(|| "<unset>".to_string());
        write!(f, "Client {{ id: {}, token: {} }}", self.client_id, token)
    }
}

fn anonymous_client_id(client: &blocking::Client) -> Result<String, Error> {
    lazy_static! {
        static ref RE_SCRIPT_TAG: Regex =
            Regex::new("<script crossorigin src=\"(.+)\"></script>").unwrap();
        static ref RE_CLIENT_ID: Regex = Regex::new("client_id:\"(.+?)\"").unwrap();
    }

    // Find the last <script> on the main page.
    let main_page_html = {
        let url = "https://soundcloud.com/discover";
        info!("querying GET {}", url);
        let mut resp = retry_execute(client, client.get(url).build()?)?.error_for_status()?;
        let mut buf = Vec::new();
        resp.copy_to(&mut buf)?;
        buf
    };
    let url = RE_SCRIPT_TAG
        .captures_iter(&main_page_html)
        .last()
        .and_then(|c| c.get(1))
        .and_then(|m| str::from_utf8(m.as_bytes()).ok())
        .ok_or(Error::Login)?;

    info!("querying GET {}", url);
    let mut main_page_resp = retry_execute(client, client.get(url).build()?)?.error_for_status()?;
    let mut buf = Vec::new();
    main_page_resp.copy_to(&mut buf)?;
    RE_CLIENT_ID
        .captures(&buf)
        .and_then(|cap| cap.get(1))
        .map(|mat| String::from_utf8_lossy(mat.as_bytes()).to_string())
        .ok_or(Error::Login)
}

// Objects used for password login.
#[derive(Serialize)]
struct PasswordLoginReqBody<'a> {
    client_id: &'a str,
    scope: &'a str,
    recaptcha_pubkey: &'a str,
    recaptcha_response: Option<&'a str>,
    credentials: Credentials<'a>,
    signature: &'a str,
    device_id: &'a str,
    user_agent: &'a str,
}

#[derive(Serialize)]
struct Credentials<'a> {
    identifier: &'a str,
    password: &'a str,
}

#[derive(Deserialize)]
struct PasswordLoginResBody {
    session: Session,
}

#[derive(Deserialize)]
struct Session {
    access_token: String,
}

#[derive(Deserialize)]
struct Page<T> {
    collection: Vec<T>,
}

impl<T: DeserializeOwned + Send> Page<T> {
    fn all_with_size_hint(
        client: &Client,
        base_url: impl AsRef<str>,
        count_hint: u64,
    ) -> Result<Vec<T>, Error> {
        let urls: Result<Vec<Url>, url::ParseError> = (0..=count_hint / PAGE_MAX_SIZE)
            .map(|num| -> Result<_, _> {
                Url::parse_with_params(
                    base_url.as_ref(),
                    &[
                        ("linked_partitioning", "1"),
                        ("limit", &format!("{}", PAGE_MAX_SIZE)),
                        ("offset", &format!("{}", num * PAGE_MAX_SIZE)),
                    ],
                )
            })
            .collect();

        let pages: Result<Vec<Page<T>>, Error> = urls?
            .into_par_iter()
            .map(|url| client.query(Method::GET, url))
            .collect();

        let all: Vec<_> = pages?
            .into_iter()
            .flat_map(|page| page.collection)
            .collect();
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_client() {
        Client::anonymous().unwrap();
    }
}
