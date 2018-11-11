use super::track::*;
use super::{Client, Error};
use reqwest::Method;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User<'a> {
    /// Integer ID
    pub id: i64,
    /// Permalink of the resource, e.g. "sbahn-sounds"
    pub permalink: String,
    /// Username, e.g. "Doctor Wilson"
    pub username: String,
    /// Last modified timestamp, e.g. "2017/09/24 09:15:49 +0000"
    pub last_modified: String,
    /// API resource URL, e.g. "http://api.soundcloud.com/comments/32562"
    pub uri: String,
    /// URL to the SoundCloud.com page, e.g. "http://soundcloud.com/bryan/sbahn-sounds"
    pub permalink_url: String,
    /// URL to a JPEG image, e.g. "http://i1.sndcdn.com/avatars-000011353294-n0axp1-large.jpg"
    pub avatar_url: String,
    /// Country, e.g. "Germany"
    pub country: String,
    /// First and last name, e.g. "Tom Wilson"
    pub full_name: String,
    /// City, e.g. "Berlin"
    pub city: String,
    /// Description, e.g. "Buskers playing in the S-Bahn station in Berlin"
    pub description: String,
    /// Discogs name, e.g. "myrandomband"
    pub discogs_name: Option<String>,
    /// MySpace name, e.g. "myrandomband"
    pub myspace_name: Option<String>,
    /// A URL to the website, e.g. "http://facebook.com/myrandomband"
    pub website: String,
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
    pub plan: String,
    // Number of private tracks
    pub private_tracks_count: i64,
    // Number of private playlists
    pub private_playlists_count: i64,
    // Boolean if email is confirmed
    pub primary_email_confirmed: bool,

    #[serde(skip_deserializing, skip_serializing)]
    client: Option<&'a Client>,
}

impl<'a> User<'a> {
    pub fn by_id(client: &Client, id: i64) -> Result<User, Error> {
        let mut rs: Result<User, _> = client.query(
            Method::GET,
            format!("https://api.soundcloud.com/users/{}", id),
        );
        if let Ok(ref mut u) = rs {
            u.client = Some(client);
        }
        rs
    }

    pub fn me(client: &Client) -> Result<User, Error> {
        let mut rs: Result<User, _> = client.query(Method::GET, "https://api.soundcloud.com/me");
        if let Ok(ref mut u) = rs {
            u.client = Some(client);
        }
        rs
    }

    pub fn favorites(&self) -> Result<Vec<Track<'a>>, Error> {
        let url = format!("https://api.soundcloud.com/users/{}/favorites", self.id);
        let mut tracks: Vec<Track> = self.client.unwrap().query(Method::GET, url)?;
        for track in &mut tracks {
            track.client = self.client;
        }
        Ok(tracks)
    }
}
