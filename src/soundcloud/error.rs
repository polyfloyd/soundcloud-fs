use reqwest;
use std::error;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "login failed")]
    Login,

    #[fail(display = "could not load client form cache: {}", _0)]
    FromCache(Box<error::Error + Send + Sync>),

    #[fail(display = "Reqwest error: {}", _0)]
    ReqwestError(reqwest::Error),
    #[fail(display = "Reqwest invalid header value: {}", _0)]
    ReqwestInvalidHeader(reqwest::header::InvalidHeaderValue),
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