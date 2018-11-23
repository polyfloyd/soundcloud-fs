use reqwest::header::{self, HeaderValue};
use reqwest::StatusCode;
use std::io;
use std::mem;

enum RangeSeekerState {
    NoResponse,
    Response(reqwest::Response),
    OutOfRange(u64),
}

pub struct RangeSeeker<'a> {
    req: reqwest::Request,
    client: &'a reqwest::Client,

    state: RangeSeekerState,
    current_offset: u64,

    // The previous request scheme is used as an optimization for file size probes.
    response_cache: Option<(reqwest::Response, u64)>,
}

impl<'a> RangeSeeker<'a> {
    pub fn new(
        client: &reqwest::Client,
        req: reqwest::Request,
    ) -> Result<RangeSeeker, reqwest::Error> {
        Ok(RangeSeeker {
            req,
            client,
            state: RangeSeekerState::NoResponse,
            current_offset: 0,
            response_cache: None,
        })
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
        let res = self
            .client
            .execute(req)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        if res.status() == StatusCode::RANGE_NOT_SATISFIABLE {
            if let Some(l) = content_length(&res) {
                self.state = RangeSeekerState::OutOfRange(l);
                return Ok(());
            } else {
                self.state = RangeSeekerState::NoResponse;
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "416 response has no Content-Length",
                ));
            }
        }

        let res = res
            .error_for_status()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        if res.status() == StatusCode::PARTIAL_CONTENT {
            self.state = RangeSeekerState::Response(res);
            return Ok(());
        }

        self.state = RangeSeekerState::NoResponse;
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

        if let RangeSeekerState::NoResponse = self.state {
            self.next_resp()?;
        }
        let res = match self.state {
            RangeSeekerState::Response(ref mut res) => res,
            RangeSeekerState::OutOfRange(_) => {
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
        if let RangeSeekerState::NoResponse = self.state {
            self.next_resp()?;
        }
        let content_length = match self.state {
            RangeSeekerState::Response(ref mut res) => {
                let content_length = content_length(res).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        "can not seek relative to end, unknown Content-Length",
                    )
                })?;
                content_length as i64
            }
            RangeSeekerState::OutOfRange(len) => len as i64,
            _ => unreachable!(),
        };

        let offset: i64 = match pos {
            io::SeekFrom::Start(offset) => offset as i64,
            io::SeekFrom::End(offset) => content_length + offset,
            io::SeekFrom::Current(offset) => self.current_offset as i64 + offset,
        };
        if offset < 0 || content_length < offset {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "http::RangeSeeker: seek position {:?} outside range 0..{}",
                    pos, content_length
                ),
            ));
        }
        let abs_offset = offset as u64;

        let mut new_state = RangeSeekerState::NoResponse;

        // io::SeekFrom::End(0) should seek to the end of the stream and causes no more bytes to be
        // read after this. We add this special case to avoid a needless HTTP request to an empty
        // body.
        if pos == io::SeekFrom::End(0) {
            new_state = RangeSeekerState::OutOfRange(content_length as u64);
        }

        if self.current_offset != abs_offset {
            // Get the previous state. This also rewrites the old state to new state so the next
            // operation will trigger a HTTP request if needed.
            mem::swap(&mut self.state, &mut new_state);
            let previous_offset = self.current_offset;
            let previous_response = match new_state {
                RangeSeekerState::Response(res) => Some(res),
                _ => None,
            };
            // If we have a cached response that has the same absolute offset as desired, reuse it.
            if self.response_cache.as_ref().map(|(_, o)| *o) == Some(abs_offset) {
                let (cached_response, _) = self.response_cache.take().unwrap();
                self.state = RangeSeekerState::Response(cached_response);
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
