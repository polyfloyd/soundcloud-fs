use reqwest::header::{self, HeaderValue};
use reqwest::StatusCode;
use std::io;

pub struct RangeSeeker<'a> {
    req: reqwest::Request,
    client: &'a reqwest::Client,

    res: Option<reqwest::Response>,
    current_offset: u64,
}

impl<'a> RangeSeeker<'a> {
    pub fn new(
        client: &reqwest::Client,
        req: reqwest::Request,
    ) -> Result<RangeSeeker, reqwest::Error> {
        Ok(RangeSeeker {
            req,
            client,
            res: None,
            current_offset: 0,
        })
    }

    fn next_resp(&self) -> io::Result<reqwest::Response> {
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
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?
            .error_for_status()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        if res.status() != StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "range request did not return Partial Content",
            ));
        }
        Ok(res)
    }
}

impl<'a> io::Read for RangeSeeker<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.res.is_none() {
            self.res = Some(self.next_resp()?);
        }
        let res = self.res.as_mut().unwrap();

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
        if self.res.is_none() {
            self.res = Some(self.next_resp()?);
        }
        let res = self.res.as_mut().unwrap();

        let content_length: i64 = res
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
        self.current_offset = abs_offset;
        Ok(abs_offset)
    }
}
