use reqwest;
use std::error;
use std::io;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "login failed")]
    Login,

    #[fail(display = "client has no token")]
    NoToken,

    #[fail(display = "downlaod not available")]
    DownloadNotAvailable,

    #[fail(display = "could not load client form cache: {}", _0)]
    FromCache(Box<error::Error + Send + Sync>),

    #[fail(display = "IO error: {}", _0)]
    IOError(io::Error),

    #[fail(display = "Reqwest error: {}", _0)]
    ReqwestError(reqwest::Error),
    #[fail(display = "Reqwest invalid header value: {}", _0)]
    ReqwestInvalidHeader(reqwest::header::InvalidHeaderValue),
    #[fail(display = "Reqwest URL parse error: {}", _0)]
    ReqwestUrlError(reqwest::UrlError),

    #[fail(
        display = "Malformed response for {} {}: {}",
        method,
        url,
        error
    )]
    MalformedResponse {
        method: reqwest::Method,
        url: reqwest::Url,
        body: String,
        error: Box<error::Error + Send + Sync>,
    },
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IOError(err)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Error {
        Error::ReqwestError(err)
    }
}

impl From<reqwest::header::InvalidHeaderValue> for Error {
    fn from(err: reqwest::header::InvalidHeaderValue) -> Error {
        Error::ReqwestInvalidHeader(err)
    }
}

impl From<reqwest::UrlError> for Error {
    fn from(err: reqwest::UrlError) -> Error {
        Error::ReqwestUrlError(err)
    }
}
