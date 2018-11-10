use soundcloud;
use time;

#[derive(Clone, Debug)]
pub enum Entry<'a> {
    User(soundcloud::User<'a>),
    UserFavorites(soundcloud::User<'a>),
    Track(soundcloud::Track),
}

impl<'a> Entry<'a> {
    pub fn file_attributes(&self, ino: u64) -> fuse::FileAttr {
        let now = time::now().to_timespec();
        match self {
            Entry::User(_user) => fuse::FileAttr {
                ino,
                size: 0,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: fuse::FileType::Directory,
                perm: 0o555,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 1,
                flags: 0,
            },
            Entry::UserFavorites(_user) => fuse::FileAttr {
                ino,
                size: 0,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: fuse::FileType::Directory,
                perm: 0o555,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 1,
                flags: 0,
            },
            Entry::Track(_track) => fuse::FileAttr {
                ino,
                size: 0,
                blocks: 1,
                atime: now,
                mtime: now,
                ctime: now,
                crtime: now,
                kind: fuse::FileType::RegularFile,
                perm: 0o444,
                nlink: 1,
                uid: 0,
                gid: 0,
                rdev: 1,
                flags: 0,
            },
        }
    }

    pub fn children(&self) -> Result<Vec<(String, Entry<'a>)>, soundcloud::Error> {
        match self {
            Entry::User(user) => Ok(vec![(
                "favorites".to_string(),
                Entry::UserFavorites(user.clone()),
            )]),
            Entry::UserFavorites(user) => {
                let children = user
                    .favorites()?
                    .into_iter()
                    .map(|track| {
                        let title = track.title.replace(char::is_whitespace, "_");
                        let name = format!("{}_{}.mp3", title, track.id);
                        (name, Entry::Track(track))
                    }).collect();
                Ok(children)
            }
            Entry::Track(_) => unreachable!("tracks do not have child files"),
        }
    }

    pub fn child_by_name(
        &self,
        child_name: impl AsRef<str>,
    ) -> Result<Option<Entry<'a>>, soundcloud::Error> {
        let child = self
            .children()?
            .into_iter()
            .find(|(name, _)| name == child_name.as_ref())
            .map(|(_, entry)| entry);
        Ok(child)
    }
}
