mod date_format;
mod error;
mod track;
mod user;
pub(crate) mod util;

extern crate chrono;
extern crate failure;
#[macro_use]
extern crate failure_derive;
extern crate id3;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate regex;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use regex::bytes::Regex;
use reqwest::{header, Url};
use serde::de::DeserializeOwned;
use std::fmt;
use std::str;

pub use self::error::Error;
pub use self::track::Track;
pub use self::user::User;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:63.0) Gecko/20100101 Firefox/63.0";

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

pub(crate) fn default_client() -> &'static reqwest::Client {
    lazy_static! {
        static ref DEFAULT_CLIENT: reqwest::Client = reqwest::Client::builder()
            .default_headers(default_headers())
            .build()
            .unwrap();
    }
    &DEFAULT_CLIENT
}

#[derive(Copy, Clone, Debug)]
pub struct Config {
    pub id3_download_images: bool,
}

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    client_id: String,
    token: Option<String>,

    pub(crate) config: Config,
}

impl Client {
    /// Set up a client by logging in using the online form, just like a user would in the web
    /// application.
    ///
    /// This login method is not guaranteed to be stable!
    pub fn login(
        config: Config,
        username: impl AsRef<str>,
        password: impl AsRef<str>,
    ) -> Result<Client, Error> {
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
            let login_res_body: PasswordLoginResBody = client
                .post(login_url)
                .json(&login_req_body)
                .send()?
                .error_for_status()?
                .json()?;
            login_res_body.session.access_token
        };

        trace!("SoundCloud login got token: {}****", &token[0..4]);
        Client::from_token(config, client_id, token)
    }

    // Attempt to create a client with read-only access to the public API.
    pub fn anonymous(config: Config) -> Result<Client, Error> {
        let client = default_client();
        let client_id = anonymous_client_id(&client)?;
        Ok(Client {
            client: client.clone(),
            client_id,
            token: None,
            config,
        })
    }

    fn from_token(
        config: Config,
        client_id: impl Into<String>,
        token: impl Into<String>,
    ) -> Result<Client, Error> {
        let token = token.into();
        let auth_client = reqwest::Client::builder()
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
            config,
        })
    }

    pub(crate) fn request(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<(reqwest::RequestBuilder, Url), Error> {
        let url = Url::parse_with_params(base_url.as_ref(), &[("client_id", &self.client_id)])?;
        let req = self.client.request(method, url.clone());
        Ok((req, url))
    }

    pub(crate) fn query<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<T, Error> {
        let (req, url) = self.request(method.clone(), base_url)?;
        info!("querying {} {}", method, url);
        let mut buf = Vec::new();
        req.send()?.error_for_status()?.copy_to(&mut buf)?;

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

fn anonymous_client_id(client: &reqwest::Client) -> Result<String, Error> {
    lazy_static! {
        static ref RE_CLIENT_ID: Regex = Regex::new("client_id:\"(.+?)\"").unwrap();
    }

    let url = "https://a-v2.sndcdn.com/assets/app-f06013d-ccf988a-3.js";
    info!("querying GET {}", url);
    let mut main_page_resp = client.get(url).send()?.error_for_status()?;
    let mut buf = Vec::new();
    main_page_resp.copy_to(&mut buf)?;
    RE_CLIENT_ID
        .captures(&buf[..])
        .and_then(|cap| cap.get(1))
        .map(|mat| String::from_utf8_lossy(mat.as_bytes()).to_string())
        .ok_or(Error::Login)
}

// Objects used for password login.
#[derive(Serialize, Deserialize)]
struct PasswordLoginReqBody<'a> {
    client_id: &'a str,
    scope: &'a str,
    recaptcha_pubkey: &'a str,
    recaptcha_response: Option<String>,
    credentials: Credentials<'a>,
    signature: &'a str,
    device_id: &'a str,
    user_agent: &'a str,
}

#[derive(Serialize, Deserialize)]
struct Credentials<'a> {
    identifier: &'a str,
    password: &'a str,
}

#[derive(Serialize, Deserialize)]
struct PasswordLoginResBody {
    session: Session,
}

#[derive(Serialize, Deserialize)]
struct Session {
    access_token: String,
}

#[derive(Deserialize)]
struct Page<T> {
    collection: Vec<T>,
    next_href: Option<String>,
}
