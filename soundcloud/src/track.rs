use super::*;
use chrono::{DateTime, Datelike, Utc};
use reqwest::{Method, Url};
use serde::{Deserialize, Deserializer};
use std::io;
use util::http;

#[derive(Clone, Debug, Deserialize)]
pub struct Track<'a> {
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
    pub purchase_url: Option<String>,
    pub download_url: Option<String>,
    //"label_id": null,
    //"purchase_title": null,
    #[serde(deserialize_with = "empty_str_as_none")]
    pub genre: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub label_name: Option<String>,
    pub release: Option<String>,
    pub track_type: Option<String>,
    pub key_signature: Option<String>,
    pub isrc: Option<String>,
    pub video_url: Option<String>,
    pub bpm: Option<i32>,
    pub release_year: Option<i32>,
    pub release_month: Option<i32>,
    pub release_day: Option<i32>,
    pub original_format: Option<String>,
    pub license: String,
    pub uri: String,
    pub user: TrackUser,
    //"attachments_uri": "https://api.soundcloud.com/tracks/515639547/attachments",
    //"user_playback_count": 1,
    //"user_favorite": true,
    pub permalink_url: String,
    pub artwork_url: Option<String>,
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

    pub fn download(&self) -> Result<impl io::Read + io::Seek + 'a, Error> {
        let sc_client = self.client.unwrap();
        if let Some(ref raw_url) = self.download_url {
            let (req_builder, _) = sc_client.request(Method::GET, Url::parse(raw_url)?)?;
            let req = req_builder.build()?;
            Ok(http::RangeSeeker::new(&sc_client.client, req)?)
        } else {
            Err(Error::DownloadNotAvailable)
        }
    }

    pub fn audio(&self) -> Result<impl io::Read + io::Seek + 'a, Error> {
        let sc_client = self.client.unwrap();
        let raw_url = format!("https://api.soundcloud.com/i1/tracks/{}/streams", self.id);
        let streams: StreamInfo = sc_client.query(Method::GET, &raw_url)?;
        let req = default_client()
            .request(Method::GET, Url::parse(&streams.http_mp3_128_url)?)
            .build()?;
        Ok(http::RangeSeeker::new(default_client(), req)?)
    }

    pub fn audio_size(&self) -> u64 {
        let bitrate = 128; // Kb/s
        (self.duration_ms * bitrate) as u64 / 8
    }

    pub fn id3_tag(&self) -> Result<impl io::Read + io::Seek, Error> {
        let mut tag = id3::Tag::new();

        tag.set_artist(self.user.username.as_str());
        tag.set_title(self.title.as_str());
        tag.set_duration(self.duration_ms as u32);
        tag.set_text("TCOP", self.license.as_str());
        tag.add_frame(id3::Frame::with_content(
            "WOAF",
            id3::Content::Link(self.permalink_url.to_string()),
        ));
        tag.add_frame(id3::Frame::with_content(
            "WOAR",
            id3::Content::Link(self.user.permalink_url.to_string()),
        ));
        tag.set_year(
            self.release_year
                .unwrap_or_else(|| self.created_at.date().year()),
        );
        if let Some(year) = self.release_year {
            tag.set_text("TORY", format!("{}", year));
        }
        if let Some(ref genre) = self.genre {
            tag.set_genre(genre.as_str());
        }
        if let Some(bpm) = self.bpm {
            tag.set_text("TBPM", format!("{}", bpm));
        }
        if let Some(label) = self.label_name.as_ref().filter(|s| !s.is_empty()) {
            tag.set_text("TPUB", label.as_str());
        }
        if let Some(isrc) = self.isrc.as_ref().filter(|s| !s.is_empty()) {
            tag.set_text("TSRC", isrc.as_str());
        }

        let enable_image = self.client.as_ref().unwrap().config.id3_download_images;
        if let Some(ref url) = self.artwork_url.as_ref().filter(|_| enable_image) {
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

            tag.add_picture(id3::frame::Picture {
                mime_type,
                picture_type: id3::frame::PictureType::CoverFront,
                description: "Artwork".to_string(),
                data,
            })
        }

        let mut id3_tag_buf = Vec::new();
        tag.write_to(&mut id3_tag_buf, id3::Version::Id3v24)
            .unwrap();
        Ok(io::Cursor::new(id3_tag_buf))
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
