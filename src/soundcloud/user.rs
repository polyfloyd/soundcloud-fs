use std::iter::{self, Iterator};

use super::track::*;

#[derive(Clone, Debug)]
pub struct User {
    id: String,
    title: String,
}

impl User {
    pub fn new(id: impl Into<String>) -> User {
        User {
            id: id.into(),
            title: "TITLE".to_string(),
        }
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    pub fn feed_tracks(&self) -> impl Iterator<Item = Track> {
        iter::once(Track::new_test())
    }
}