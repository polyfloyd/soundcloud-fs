mod error;
mod track;
mod user;

use self::error::*;
use reqwest;
use reqwest::{header, Url};

pub use self::track::Track;
pub use self::user::User;

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:63.0) Gecko/20100101 Firefox/63.0";

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

        let client_id = "Ine5MMVzbMYXUSWyEkyHNWzC7p8wKpzb";

        trace!("performing password login with user: {}", username.as_ref());
        let login_req_body = PasswordLoginReqBody {
            client_id,
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
            &[("client_id", client_id)],
        ).unwrap();
        trace!("password login URL: {}", login_url);
        let login_res_body: PasswordLoginResBody = client
            .post(login_url)
            .json(&login_req_body)
            .send()?
            .error_for_status()?
            .json()?;
        let token = login_res_body.session.access_token;

        let auth_client = reqwest::Client::builder()
            .default_headers({
                let auth_header = format!("OAuth {}", token).parse()?;
                let mut headers = default_headers();
                headers.insert(header::AUTHORIZATION, auth_header);
                headers
            }).build()?;

        trace!("SoundCloud login got token: {}****", &token[0..4]);
        Ok(Client {
            client,
            client_id: client_id.to_string(),
            token,
        })
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
