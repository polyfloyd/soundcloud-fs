use super::*;
use super::{Client, Error};
use chrono::{DateTime, Utc};
use reqwest::Method;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    /// Integer ID
    pub id: i64,
    /// Permalink of the resource, e.g. "sbahn-sounds"
    pub permalink: String,
    /// Username, e.g. "Doctor Wilson"
    pub username: String,
    /// Last modified timestamp, e.g. "2017/09/24 09:15:49 +0000"
    #[serde(with = "date_format")]
    pub last_modified: DateTime<Utc>,
    /// API resource URL, e.g. "http://api.soundcloud.com/comments/32562"
    pub uri: String,
    /// URL to the SoundCloud.com page, e.g. "http://soundcloud.com/bryan/sbahn-sounds"
    pub permalink_url: String,
    /// URL to a JPEG image, e.g. "http://i1.sndcdn.com/avatars-000011353294-n0axp1-large.jpg"
    pub avatar_url: String,
    /// Country, e.g. "Germany"
    pub country: Option<String>,
    /// First and last name, e.g. "Tom Wilson"
    pub full_name: String,
    /// City, e.g. "Berlin"
    pub city: Option<String>,
    /// Description, e.g. "Buskers playing in the S-Bahn station in Berlin"
    pub description: Option<String>,
    /// Discogs name, e.g. "myrandomband"
    pub discogs_name: Option<String>,
    /// MySpace name, e.g. "myrandomband"
    pub myspace_name: Option<String>,
    /// A URL to the website, e.g. "http://facebook.com/myrandomband"
    pub website: Option<String>,
    /// A custom title for the website, e.g. "myrandomband on Facebook"
    pub website_title: Option<String>,
    /// Online status
    pub online: bool,
    /// Number of public tracks
    pub track_count: i64,
    /// Number of public playlists
    pub playlist_count: i64,
    // Number of followers
    pub followers_count: i64,
    // Number of followed users
    pub followings_count: i64,
    // Number of favorited public tracks
    pub public_favorites_count: i64,
    // Subscription plan of the user, e.g. "Pro Plus"
    pub plan: Option<String>,
    // Number of private tracks
    pub private_tracks_count: Option<i64>,
    // Number of private playlists
    pub private_playlists_count: Option<i64>,
    // Boolean if email is confirmed
    pub primary_email_confirmed: Option<bool>,
}

impl User {
    pub fn by_name(client: &Client, name: impl AsRef<str>) -> Result<User, Error> {
        client.query(
            Method::GET,
            format!("https://api.soundcloud.com/users/{}", name.as_ref()),
        )
    }

    pub fn me(client: &Client) -> Result<User, Error> {
        client.query(Method::GET, "https://api.soundcloud.com/me")
    }

    pub fn tracks(&self, client: &Client) -> Result<Vec<Track>, Error> {
        let url = format!("https://api.soundcloud.com/users/{}/tracks", self.id);
        Page::all_with_size_hint(client, url, self.track_count as u64)
    }

    pub fn favorites(&self, client: &Client) -> Result<Vec<Track>, Error> {
        let url = format!("https://api.soundcloud.com/users/{}/favorites", self.id);
        Page::all_with_size_hint(client, url, self.public_favorites_count as u64)
    }

    pub fn following(&self, client: &Client) -> Result<Vec<User>, Error> {
        let url = format!("https://api.soundcloud.com/users/{}/followings", self.id);
        Page::all_with_size_hint(client, url, self.followings_count as u64)
    }
}

impl Hash for User {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}
