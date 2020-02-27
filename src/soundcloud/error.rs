use reqwest;
use std::error;
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Login,
    ArtworkNotAvailable,

    IOError(io::Error),

    ReqwestError(reqwest::Error),
    ReqwestInvalidHeader(reqwest::header::InvalidHeaderValue),
    ReqwestUrlParseError(url::ParseError),

    MalformedResponse {
        method: reqwest::Method,
        url: reqwest::Url,
        body: String,
        error: Box<dyn error::Error + Send + Sync>,
    },

    Generic(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::IOError(err)
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        Self::ReqwestError(err)
    }
}

impl From<reqwest::header::InvalidHeaderValue> for Error {
    fn from(err: reqwest::header::InvalidHeaderValue) -> Self {
        Self::ReqwestInvalidHeader(err)
    }
}

impl From<url::ParseError> for Error {
    fn from(err: url::ParseError) -> Self {
        Self::ReqwestUrlParseError(err)
    }
}
