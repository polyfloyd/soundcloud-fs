use log::*;
use reqwest::header::{self, HeaderValue};
use reqwest::StatusCode;
use std::io;
use std::mem;

enum State {
    NoResponse,
    Response(Box<reqwest::Response>),
    OutOfRange,
}

pub struct RangeSeeker<'a> {
    client: &'a reqwest::Client,
    req: reqwest::Request,
    num_requests: u64,

    state: State,
    current_offset: u64,
    content_length: Option<u64>,

    // The previous request scheme is used as an optimization for file size probes.
    response_cache: Option<(Box<reqwest::Response>, u64)>,
}

impl<'a> RangeSeeker<'a> {
    pub fn new(client: &'a reqwest::Client, req: reqwest::Request) -> Self {
        RangeSeeker {
            client,
            req,
            num_requests: 0,
            state: State::NoResponse,
            current_offset: 0,
            content_length: None,
            response_cache: None,
        }
    }

    fn next_resp(&mut self) -> io::Result<()> {
        let mut req = reqwest::Request::new(self.req.method().clone(), self.req.url().clone());
        req.headers_mut().insert(
            header::RANGE,
            HeaderValue::from_str(&format!("bytes={}-", self.current_offset))
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?,
        );

        info!(
            "querying {} {} (offset: {})",
            req.method(),
            req.url(),
            self.current_offset
        );
        self.num_requests += 1;
        let res = self
            .client
            .execute(req)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        if res.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            let o = self.current_offset;
            self.current_offset = 0;
            self.next_resp()?;
            self.state = State::OutOfRange;
            self.current_offset = o;
            return Ok(());
        }

        let res = res
            .error_for_status()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        if (self.current_offset == 0 && res.status() == StatusCode::OK)
            || res.status() == StatusCode::PARTIAL_CONTENT
        {
            let clen = content_length(&res).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "response did not include Content-Length",
                )
            })?;
            self.content_length = Some(self.current_offset + clen);
            self.state = State::Response(Box::new(res));
            return Ok(());
        }

        self.state = State::NoResponse;
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "range request did not return Partial Content, got status {}",
                res.status()
            ),
        ))
    }
}

impl<'a> io::Read for RangeSeeker<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Drop any cached responses to avoid leaking connections.
        self.response_cache = None;

        if let Some(l) = self.content_length {
            if self.current_offset >= l {
                return Ok(0);
            }
        }

        if let State::NoResponse = self.state {
            self.next_resp()?;
        }
        let res = match self.state {
            State::Response(ref mut res) => res,
            State::OutOfRange => {
                return Ok(0);
            }
            _ => unreachable!(),
        };

        let mut nread = 0;
        let mut n = 1;
        while !buf.is_empty() && n > 0 {
            n = res.read(&mut buf[nread..])?;
            nread += n;
        }
        self.current_offset += nread as u64;
        Ok(nread)
    }
}

impl<'a> io::Seek for RangeSeeker<'a> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let abs_offset = match pos {
            io::SeekFrom::Start(offset) => offset,
            io::SeekFrom::End(offset) => {
                if self.content_length.is_none() {
                    self.next_resp()?;
                }
                valid_offset(self.content_length.unwrap() as i64 + offset)?
            }
            io::SeekFrom::Current(offset) => valid_offset(self.current_offset as i64 + offset)?,
        };

        let mut new_state = if pos == io::SeekFrom::End(0) {
            // io::SeekFrom::End(0) should seek to the end of the stream and causes no more bytes
            // to be read after this. We add this special case to avoid a needless HTTP request to
            // an empty body.
            State::OutOfRange
        } else {
            State::NoResponse
        };

        if self.current_offset != abs_offset {
            // Get the previous state. This also rewrites the old state to new state so the next
            // operation will trigger a HTTP request if needed.
            mem::swap(&mut self.state, &mut new_state);
            let previous_offset = self.current_offset;
            let previous_response = match new_state {
                State::Response(res) => Some(res),
                _ => None,
            };
            // If we have a cached response that has the same absolute offset as desired, reuse it.
            if self.response_cache.as_ref().map(|(_, o)| *o) == Some(abs_offset) {
                let (cached_response, _) = self.response_cache.take().unwrap();
                self.state = State::Response(cached_response);
            }
            // Cache the old response so we can reuse it later.
            if let Some(res) = previous_response {
                self.response_cache = Some((res, previous_offset));
            }
        }
        self.current_offset = abs_offset;
        Ok(abs_offset)
    }
}

fn content_length(res: &reqwest::Response) -> Option<u64> {
    res.headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|ct_len| ct_len.to_str().ok())
        .and_then(|ct_len| ct_len.parse().ok())
}

fn valid_offset(offset: i64) -> Result<u64, io::Error> {
    if offset < 0 {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("http::RangeSeeker: can not seek to {}", offset),
        ));
    }
    Ok(offset as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};

    fn test_request(size: usize) -> reqwest::Request {
        // Ideally, we are not dependant on an external server for unit tests...
        let mut req = reqwest::Request::new(
            reqwest::Method::GET,
            format!("https://httpbin.org/range/{}", size)
                .parse()
                .unwrap(),
        );
        req.headers_mut().insert(
            header::ACCEPT,
            HeaderValue::from_static("application/octet-stream"),
        );
        req
    }

    fn test_request_resp(start: usize, end: usize) -> Vec<u8> {
        (97..=122)
            .into_iter()
            .cycle()
            .skip(start)
            .take(end - start)
            .collect()
    }

    #[test]
    fn test_read_all() {
        const SIZE: usize = 8192;
        let client = reqwest::Client::new();
        let req = test_request(SIZE);

        let mut f = RangeSeeker::new(&client, req);

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert_eq!(test_request_resp(0, SIZE), buf);

        assert_eq!(1, f.num_requests);
    }

    #[test]
    fn test_read_partial() {
        const SIZE: usize = 8192;
        let client = reqwest::Client::new();
        let req = test_request(SIZE);

        let mut f = RangeSeeker::new(&client, req);

        let new_pos = f.seek(io::SeekFrom::Start(4000)).unwrap();
        assert_eq!(4000, new_pos);

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert_eq!(test_request_resp(4000, SIZE), buf);

        assert_eq!(1, f.num_requests);
    }

    #[test]
    fn test_seek_to_end() {
        const SIZE: usize = 8192;
        let client = reqwest::Client::new();
        let req = test_request(SIZE);

        let mut f = RangeSeeker::new(&client, req);

        let new_pos = f.seek(io::SeekFrom::End(0)).unwrap();
        assert_eq!(SIZE as u64, new_pos);

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert!(buf.is_empty());

        assert_eq!(1, f.num_requests);
    }

    #[test]
    fn test_probe_size() {
        const SIZE: usize = 8192;
        let client = reqwest::Client::new();
        let req = test_request(SIZE);

        let mut f = RangeSeeker::new(&client, req);

        let new_pos = f.seek(io::SeekFrom::End(0)).unwrap();
        assert_eq!(SIZE as u64, new_pos);

        let new_pos = f.seek(io::SeekFrom::Start(0)).unwrap();
        assert_eq!(0, new_pos);

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert_eq!(test_request_resp(0, SIZE), buf);

        assert_eq!(1, f.num_requests);
    }

    #[test]
    fn test_read_after_seek() {
        const SIZE: usize = 8192;
        let client = reqwest::Client::new();
        let req = test_request(SIZE);

        let mut f = RangeSeeker::new(&client, req);

        let new_pos = f.seek(io::SeekFrom::End(-100)).unwrap();
        assert_eq!(SIZE as u64 - 100, new_pos);
        let mut buf = [0; 50];
        f.read_exact(&mut buf).unwrap();

        let new_pos = f.seek(io::SeekFrom::End(-10)).unwrap();
        assert_eq!(SIZE as u64 - 10, new_pos);

        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert_eq!(test_request_resp(SIZE - 10, SIZE), buf);
    }
}
