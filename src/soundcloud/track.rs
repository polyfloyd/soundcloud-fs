use super::{Client, Error};
use reqwest::Url;
use std::io;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track<'a> {
    pub id: i64,
    pub created_at: String,
    pub user_id: i64,
    // Duration in milliseconds
    pub duration: i64,
    pub commentable: bool,
    pub state: String,
    //"original_content_size": 217721123,
    pub last_modified: String,
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
    pub genre: String,
    pub title: String,
    pub description: String,
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
    //"original_format": "mp3",
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
    pub fn audio_accessible(&self) -> bool {
        self.download_url.is_some()
    }

    pub fn audio(&self) -> Result<impl io::Read, Error> {
        if let Some(raw_url) = self.download_url.as_ref() {
            let res = self
                .client
                .unwrap()
                .client
                .get(Url::parse(raw_url)?)
                .send()?
                .error_for_status()?;
            Ok(res)
        } else {
            Err(Error::AudioNotAccessible)
        }
    }
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
