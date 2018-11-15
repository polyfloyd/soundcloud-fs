use super::*;
use chrono::{DateTime, Utc};
use reqwest::header::{self, HeaderValue};
use reqwest::{Method, StatusCode, Url};
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track<'a> {
    pub id: i64,
    #[serde(with = "date_format")]
    pub created_at: DateTime<Utc>,
    pub user_id: i64,
    // Duration in milliseconds
    pub duration: i64,
    pub commentable: bool,
    pub state: String,
    pub original_content_size: u64,
    #[serde(with = "date_format")]
    pub last_modified: DateTime<Utc>,
    pub sharing: String,
    pub tag_list: String,
    pub permalink: String,
    pub streamable: bool,
    pub embeddable_by: String,
    pub downloadable: bool,
    pub purchase_url: Option<String>,
    pub download_url: Option<String>,
    //"label_id": null,
    //"purchase_title": null,
    pub genre: Option<String>,
    pub title: String,
    pub description: Option<String>,
    //"label_name": null,
    //"release": null,
    //"track_type": null,
    //"key_signature": null,
    //"isrc": null,
    //"video_url": null,
    //"bpm": null,
    //"release_year": null,
    //"release_month": null,
    //"release_day": null,
    pub original_format: Option<String>,
    //"license": "all-rights-reserved",
    pub uri: String,
    pub user: TrackUser,
    //"attachments_uri": "https://api.soundcloud.com/tracks/515639547/attachments",
    //"user_playback_count": 1,
    //"user_favorite": true,
    //"permalink_url": "http://soundcloud.com/theviicrew/vii-radio-024-john-askew",
    //"artwork_url": "https://i1.sndcdn.com/artworks-000422755914-93c8y9-large.jpg",
    //"waveform_url": "https://w1.sndcdn.com/17huh4rFYXFb_m.png",
    //"stream_url": "https://api.soundcloud.com/tracks/515639547/stream",
    //"playback_count": 0,
    //"download_count": 0,
    //"favoritings_count": 384,
    //"comment_count": 31,
    //"likes_count": 384,
    //"reposts_count": 0,
    //"policy": "ALLOW",
    //"monetization_model": "NOT_APPLICABLE"
    #[serde(skip_deserializing, skip_serializing)]
    pub(crate) client: Option<&'a Client>,
}

impl<'a> Track<'a> {
    pub fn audio_format(&self) -> &str {
        if self.download_url.is_none() {
            return "mp3";
        }
        match self.original_format.as_ref().map(|s| s.as_str()) {
            Some("raw") => "mp3",
            Some(s) => s,
            None => "mp3",
        }
    }

    pub fn audio(&self) -> Result<impl io::Read + io::Seek + 'a, Error> {
        let sc_client = self.client.unwrap();

        if let Some(raw_url) = self.download_url.as_ref() {
            info!("accessing audio through the download URL");
            let (req_builder, _) = sc_client.request(Method::GET, Url::parse(raw_url)?)?;
            let req = req_builder.build()?;
            return RangeSeeker::new(&sc_client.client, req);
        }

        info!("accessing audio through the streams API");
        lazy_static! {
            static ref DEFAULT_CLIENT: reqwest::Client = reqwest::Client::builder()
                .default_headers(default_headers())
                .build()
                .unwrap();
        }
        let raw_url = format!("https://api.soundcloud.com/i1/tracks/{}/streams", self.id);
        let streams: StreamInfo = sc_client.query(Method::GET, &raw_url)?;
        let req = DEFAULT_CLIENT
            .request(Method::GET, Url::parse(&streams.http_mp3_128_url)?)
            .build()?;
        RangeSeeker::new(&DEFAULT_CLIENT, req)
    }
}

#[derive(Deserialize)]
struct StreamInfo {
    http_mp3_128_url: String,
    //hls_mp3_128_url: String,
    //hls_opus_64_url: String,
    //preview_mp3_128_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackUser {
    id: i64,
    permalink: String,
    username: String,
    last_modified: String,
    uri: String,
    permalink_url: String,
    avatar_url: String,
}

struct RangeSeeker<'a> {
    req: reqwest::Request,
    res: reqwest::Response,
    current_offset: u64,
    client: &'a reqwest::Client,
}

impl<'a> RangeSeeker<'a> {
    fn new(client: &reqwest::Client, req: reqwest::Request) -> Result<RangeSeeker, Error> {
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
