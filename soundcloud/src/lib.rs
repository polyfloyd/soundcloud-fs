mod error;
mod track;
mod user;

extern crate failure;
#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate log;
extern crate reqwest;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate time;

use reqwest::{header, Url};
use serde::de::DeserializeOwned;
use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::str;

pub use self::error::Error;
pub use self::track::Track;
pub use self::user::User;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:63.0) Gecko/20100101 Firefox/63.0";
const CLIENT_ID: &str = "Ine5MMVzbMYXUSWyEkyHNWzC7p8wKpzb";

fn default_headers() -> header::HeaderMap {
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

#[derive(Clone)]
pub struct Client {
    client: reqwest::Client,
    client_id: String,
    token: String,
}

impl Client {
    /// Set up a client by logging in using the online form, just like a user would in the web
    /// application.
    ///
    /// This login method is not guaranteed to be stable!
    pub fn login(username: impl AsRef<str>, password: impl AsRef<str>) -> Result<Client, Error> {
        let client = reqwest::Client::builder()
            .default_headers(default_headers())
            .build()?;

        trace!("performing password login with user: {}", username.as_ref());
        let login_req_body = PasswordLoginReqBody {
            client_id: CLIENT_ID,
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
            &[("client_id", CLIENT_ID)],
        ).unwrap();
        trace!("password login URL: {}", login_url);
        let login_res_body: PasswordLoginResBody = client
            .post(login_url)
            .json(&login_req_body)
            .send()?
            .error_for_status()?
            .json()?;

        let token = login_res_body.session.access_token;
        trace!("SoundCloud login got token: {}****", &token[0..4]);
        Client::from_token(token)
    }

    pub fn from_token(token: impl Into<String>) -> Result<Client, Error> {
        let token = token.into();
        let auth_client = reqwest::Client::builder()
            .default_headers({
                let auth_header = format!("OAuth {}", token).parse()?;
                let mut headers = default_headers();
                headers.insert(header::AUTHORIZATION, auth_header);
                headers
            }).build()?;
        Ok(Client {
            client: auth_client,
            client_id: CLIENT_ID.to_string(),
            token: token.into(),
        })
    }

    pub fn from_cache(filename: impl AsRef<Path>) -> Result<Client, Error> {
        let raw_token = fs::read(filename).map_err(|err| Error::FromCache(Box::new(err)))?;
        let token =
            str::from_utf8(&raw_token[..]).map_err(|err| Error::FromCache(Box::new(err)))?;
        Client::from_token(token)
    }

    pub fn cache_to(&self, filename: impl AsRef<Path>) -> io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .mode(0o600)
            .write(true)
            .create(true)
            .open(filename)?;
        f.write_all(self.token.as_bytes())?;
        Ok(())
    }

    pub fn query<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        base_url: impl AsRef<str>,
    ) -> Result<T, Error> {
        let url = Url::parse_with_params(base_url.as_ref(), &[("client_id", &self.client_id)])?;
        info!("querying {} {}", method, url);

        let mut buf = Vec::new();
        self.client
            .request(method.clone(), url.clone())
            .send()?
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
        write!(
            f,
            "Client {{ id: {}, token: {}**** }}",
            self.client_id,
            &self.token[0..4]
        )
    }
}

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

// Common API headers
//
// GET /me/play-history/tracks?client_id=Ine5MMVzbMYXUSWyEkyHNWzC7p8wKpzb&limit=25&offset=0&linked_partitioning=1&app_version=1541509103&app_locale=en HTTP/1.1
// Host: api-v2.soundcloud.com
// User-Agent: Mozilla/5.0 (X11; Linux x86_64; rv:63.0) Gecko/20100101 Firefox/63.0
// Accept: application/json, text/javascript, */*; q=0.01
// Accept-Language: en-US,en;q=0.5
// Accept-Encoding: gzip, deflate, br
// Referer: https://soundcloud.com/
// Authorization: OAuth 2-283605-41912219-YwFGFMHKimIMpY7
// Origin: https://soundcloud.com
// DNT: 1
// Connection: keep-alive
// Cache-Control: max-age=0

// Login procedure
//
//POST https://api-v2.soundcloud.com/sign-in/password?client_id=Ine5MMVzbMYXUSWyEkyHNWzC7p8wKpzb&app_version=1541509103&app_locale=en
//{
//  "client_id":"Ine5MMVzbMYXUSWyEkyHNWzC7p8wKpzb",
//  "scope":"fast-connect non-expiring purchase signup upload",
//  "recaptcha_pubkey":"6LeAxT8UAAAAAOLTfaWhndPCjGOnB54U1GEACb7N",
//  "recaptcha_response":null,
//  "credentials":{
//    "identifier":"USERNAME",
//    "password":"PASSWORD"
//  },
//  "signature":"8:3-1-28405-134-1638720-1024-0-0:4ab691:2",
//  "device_id":"381629-667600-267798-887023",
//  "user_agent":"Mozilla/5.0 (X11; Linux x86_64; rv:63.0) Gecko/20100101 Firefox/63.0"
//}
//
//{
// "session":{
//   "access_token":"TOKEN"
//  }
//}
