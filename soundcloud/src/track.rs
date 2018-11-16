use super::*;
use chrono::{DateTime, Utc};
use reqwest::{Method, Url};
use std::io;
use util::http;

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
            return Ok(http::RangeSeeker::new(&sc_client.client, req)?);
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
        Ok(http::RangeSeeker::new(&DEFAULT_CLIENT, req)?)
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
