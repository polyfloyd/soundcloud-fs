use super::format;
use crate::soundcloud::util::http;
use crate::soundcloud::*;
use chrono::{DateTime, Utc};
use reqwest::Method;
use std::hash::{Hash, Hasher};
use std::io;

const AUDIO_CBR_BITRATE: u64 = 128_000;

#[derive(Clone, Debug, Deserialize)]
pub struct Track {
    pub id: i64,
    #[serde(with = "format::date")]
    pub created_at: DateTime<Utc>,
    pub user_id: i64,
    #[serde(rename = "duration")]
    pub duration_ms: i64,
    #[serde(with = "format::null_as_false")]
    pub commentable: bool,
    pub state: String,
    pub original_content_size: u64,
    #[serde(with = "format::date")]
    pub last_modified: DateTime<Utc>,
    pub sharing: String,
    pub tag_list: String,
    pub permalink: String,
    #[serde(with = "format::null_as_false")]
    pub streamable: bool,
    pub embeddable_by: String,
    #[serde(with = "format::null_as_false")]
    pub downloadable: bool,
    #[serde(default, with = "format::empty_str_as_none")]
    pub purchase_url: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub download_url: Option<String>,
    //"label_id": null,
    //"purchase_title": null,
    #[serde(default, with = "format::empty_str_as_none")]
    pub genre: Option<String>,
    pub title: String,
    #[serde(default, with = "format::empty_str_as_none")]
    pub description: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub label_name: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub release: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub track_type: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub key_signature: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub isrc: Option<String>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub video_url: Option<String>,
    pub bpm: Option<f32>,
    pub release_year: Option<i32>,
    pub release_month: Option<i32>,
    pub release_day: Option<i32>,
    #[serde(default, with = "format::empty_str_as_none")]
    pub original_format: Option<String>,
    pub license: String,
    pub uri: String,
    pub user: TrackUser,
    //"attachments_uri": "https://api.soundcloud.com/tracks/515639547/attachments",
    //"user_playback_count": 1,
    //"user_favorite": true,
    pub permalink_url: String,
    #[serde(default, with = "format::empty_str_as_none")]
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
    #[cfg(test)]
    pub fn by_id(client: &Client, id: i64) -> Result<Self, Error> {
        let url = format!("https://api.soundcloud.com/tracks/{}", id);
        client.query(Method::GET, url)
    }

    pub fn audio<'a>(&self, client: &'a Client) -> Result<impl io::Read + io::Seek + 'a, Error> {
        lazy_static! {
            static ref RE_HLS_URL: regex::Regex =
                regex::Regex::new("https://[^\"]+?/stream/hls").unwrap();
            static ref RE_MP3_URL: regex::Regex =
                regex::Regex::new("^(.+/media)/(\\d+)/(\\d+)/(.+)$").unwrap();
        }
        // Query the track's HTML page, we need to find a URL ending with `/hls` to follow.
        let html_page = client.query_string(Method::GET, &self.permalink_url)?;
        let hls_url = RE_HLS_URL
            .find(&html_page)
            .map(|m| m.as_str())
            .ok_or_else(|| Error::Generic("hls url not found on page".to_string()))?;
        // Query the URL, the returned object contains another URL which points to a playlist file.
        let hls_info: HLSInfo = client.query(Method::GET, hls_url)?;
        // Get the playlist file.
        let playlist_file = retry_execute(
            default_client(),
            default_client().get(&hls_info.url).build()?,
        )?
        .text()?;
        // The playlist is in M3U format. Each entry in this playlist is a successive part of the
        // full audio file.
        let mp3_files: Vec<_> = playlist_file
            .lines()
            // Lines starting with `#` are metadata.
            .filter(|line| !line.starts_with('#'))
            .collect();
        // Hack: Concatenate the files by rewriting the offsets. The offsets are the
        // `/media/<start>/<end>` part of the URL.
        let last_mp3 = mp3_files
            .last()
            .ok_or_else(|| Error::Generic("no files in track playlist".to_string()))?;
        let cap = RE_MP3_URL
            .captures(last_mp3)
            .ok_or_else(|| Error::Generic("unexpected MP3 url format".to_string()))?;
        let mp3_url = format!("{}/{}/{}/{}", &cap[1], 0, &cap[3], &cap[4]);
        let req = default_client().get(&mp3_url).build()?;
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
        let mut resp = retry_execute(default_client(), default_client().get(&url).build()?)?
            .error_for_status()?;

        let mime_type = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .map(ToString::to_string)
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

#[derive(Deserialize, Debug)]
struct HLSInfo {
    url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TrackUser {
    pub id: i64,
    pub permalink: String,
    pub username: String,
    pub last_modified: String,
    pub uri: String,
    pub permalink_url: String,
    pub avatar_url: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use io::Read;

    #[test]
    fn get_audio() {
        // https://soundcloud.com/wright-and-bastard/the-fat-dandy-butterfly-slims
        // CC BY-NC-SA 3.0
        let id = 609233313;

        let client = Client::anonymous().unwrap();
        let track = Track::by_id(&client, id).unwrap();

        let mut r = track.audio(&client).unwrap();
        let mut b = [0; 4096];
        r.read_exact(&mut b[..]).unwrap();
    }
}
