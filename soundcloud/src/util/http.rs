use reqwest::header::{self, HeaderValue};
use reqwest::StatusCode;
use std::io;

pub struct RangeSeeker<'a> {
    req: reqwest::Request,
    res: reqwest::Response,
    current_offset: u64,
    client: &'a reqwest::Client,
}

impl<'a> RangeSeeker<'a> {
    pub fn new(
        client: &reqwest::Client,
        req: reqwest::Request,
    ) -> Result<RangeSeeker, reqwest::Error> {
        let res = client.execute(clone_request(&req))?.error_for_status()?;
        Ok(RangeSeeker {
            req,
            res,
            current_offset: 0,
            client,
        })
    }
}

impl<'a> io::Read for RangeSeeker<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut nread = 0;
        let mut n = 1;
        while !buf.is_empty() && n > 0 {
            n = self.res.read(&mut buf[nread..])?;
            nread += n;
        }
        self.current_offset += nread as u64;
        Ok(nread)
    }
}

impl<'a> io::Seek for RangeSeeker<'a> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let content_length: i64 = self
            .res
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|ct_len| ct_len.to_str().ok())
            .and_then(|ct_len| ct_len.parse().ok())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::Other,
                    "can not seek relative to end, unknown Content-Length",
                )
            })?;
        let offset: i64 = match pos {
            io::SeekFrom::Start(offset) => offset as i64,
            io::SeekFrom::End(offset) => content_length + offset,
            io::SeekFrom::Current(offset) => self.current_offset as i64 + offset,
        };
        if offset < 0 || content_length <= offset {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "seek position {:?} outside range 0..{}",
                    pos, content_length
                ),
            ));
        }
        let abs_offset = offset as u64;
        if abs_offset == self.current_offset {
            return Ok(abs_offset);
        }

        info!(
            "querying {} {} (offset: {})",
            self.req.method(),
            self.req.url(),
            abs_offset
        );
        let mut req = reqwest::Request::new(self.req.method().clone(), self.req.url().clone());
        req.headers_mut().insert(
            header::RANGE,
            HeaderValue::from_str(&format!("bytes={}-", abs_offset))
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?,
        );

        let res = self
            .client
            .execute(req)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?
            .error_for_status()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        if res.status() != StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "range request did not return Partial Content",
            ));
        }

        self.current_offset = abs_offset;
        self.res = res;
        Ok(abs_offset)
    }
}

fn clone_request(orig: &reqwest::Request) -> reqwest::Request {
    let mut req = reqwest::Request::new(orig.method().clone(), orig.url().clone());
    for (k, v) in orig.headers() {
        req.headers_mut().insert(k, v.clone());
    }
    req
}
