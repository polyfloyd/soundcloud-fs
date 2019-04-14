use crate::util::http;
use crate::*;
use chrono::{DateTime, Utc};
use reqwest::{Method, Url};
use serde::{Deserialize, Deserializer};
use std::hash::{Hash, Hasher};
use std::io;

const AUDIO_CBR_BITRATE: u64 = 128_000;

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: i64,
    #[serde(with = "date_format")]
    pub created_at: DateTime<Utc>,
    pub user_id: i64,
    #[serde(rename = "duration")]
    pub duration_ms: i64,
    #[serde(deserialize_with = "null_as_false")]
    pub commentable: bool,
    pub state: String,
    pub original_content_size: u64,
    #[serde(with = "date_format")]
    pub last_modified: DateTime<Utc>,
    pub sharing: String,
    pub tag_list: String,
    pub permalink: String,
    #[serde(deserialize_with = "null_as_false")]
    pub streamable: bool,
    pub embeddable_by: String,
    #[serde(deserialize_with = "null_as_false")]
    pub downloadable: bool,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub purchase_url: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub download_url: Option<String>,
    //"label_id": null,
    //"purchase_title": null,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub genre: Option<String>,
    pub title: String,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub description: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub label_name: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub release: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub track_type: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub key_signature: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub isrc: Option<String>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub video_url: Option<String>,
    pub bpm: Option<f32>,
    pub release_year: Option<i32>,
    pub release_month: Option<i32>,
    pub release_day: Option<i32>,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub original_format: Option<String>,
    pub license: String,
    pub uri: String,
    pub user: TrackUser,
    //"attachments_uri": "https://api.soundcloud.com/tracks/515639547/attachments",
    //"user_playback_count": 1,
    //"user_favorite": true,
    pub permalink_url: String,
    #[serde(deserialize_with = "empty_str_as_none")]
    artwork_url: Option<String>,
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
}

impl Track {
    pub fn download_format(&self) -> &str {
        if self.download_url.is_none() {
            return "mp3";
        }
        match self.original_format.as_ref().map(|s| s.as_str()) {
            Some("raw") => "mp3",
            Some(s) => s,
            None => "mp3",
        }
    }

    pub fn download<'a>(&self, client: &'a Client) -> Result<impl io::Read + io::Seek + 'a, Error> {
        if let Some(ref raw_url) = self.download_url {
            let (req_builder, _) = client.request(Method::GET, Url::parse(raw_url)?)?;
            let req = req_builder.build()?;
            Ok(http::RangeSeeker::new(&client.client, req))
        } else {
            Err(Error::DownloadNotAvailable)
        }
    }

    pub fn audio<'a>(&self, client: &'a Client) -> Result<impl io::Read + io::Seek + 'a, Error> {
        let raw_url = format!("https://api.soundcloud.com/i1/tracks/{}/streams", self.id);
        let streams: StreamInfo = client.query(Method::GET, &raw_url)?;
        let req = default_client()
            .request(Method::GET, Url::parse(&streams.http_mp3_128_url)?)
            .build()?;
        Ok(http::RangeSeeker::new(default_client(), req))
    }

    pub fn audio_size(&self) -> u64 {
        self.duration_ms as u64 * AUDIO_CBR_BITRATE / 1000 / 8
    }

    pub fn artwork(&self) -> Result<(Vec<u8>, String), Error> {
        let url = match &self.artwork_url {
            Some(v) => v,
            None => return Err(Error::ArtworkNotAvailable),
        };

        // "large.jpg" is actually a 100x100 image. We can tweak the URL to point to a larger
        // image instead.
        let url = if url.ends_with("-large.jpg") {
            format!("{}-t500x500.jpg", url.trim_end_matches("-large.jpg"))
        } else {
            url.to_string()
        };

        info!("querying GET {}", url);
        let mut resp = default_client().get(&url).send()?.error_for_status()?;

        let mime_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .map(|h| h.to_string())
            .unwrap_or_else(|| "image/jpg".to_string());
        let mut data = Vec::new();
        resp.copy_to(&mut data)?;
        Ok((data, mime_type))
    }
}

impl Hash for Track {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
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
    pub id: i64,
    pub permalink: String,
    pub username: String,
    pub last_modified: String,
    pub uri: String,
    pub permalink_url: String,
    pub avatar_url: String,
}

fn empty_str_as_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let o: Option<String> = Option::deserialize(d)?;
    Ok(o.filter(|s| !s.is_empty()))
}

fn null_as_false<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let o: Option<bool> = Option::deserialize(d)?;
    Ok(o.unwrap_or(false))
}
