use soundcloud;
use time;

#[derive(Clone, Debug)]
pub enum Entry {
    User(soundcloud::User),
    UserFeed(soundcloud::User),
    Track(soundcloud::Track),
}

impl Entry {
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
            Entry::UserFeed(_user) => fuse::FileAttr {
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

    pub fn children(&self) -> Box<Iterator<Item = (String, Entry)>> {
        match self {
            Entry::User(user) => {
                Box::new(vec![("feed".to_string(), Entry::UserFeed(user.clone()))].into_iter())
            }
            Entry::UserFeed(user) => {
                let iter = user
                    .feed_tracks()
                    .map(|track| (track.id() + ".mp3", Entry::Track(track)));
                Box::new(iter)
            }
            Entry::Track(_) => unreachable!("tracks do not have child files"),
        }
    }

    pub fn child_by_name(&self, child_name: impl AsRef<str>) -> Option<Entry> {
        self.children()
            .find(|(name, _)| name == child_name.as_ref())
            .map(|(_, entry)| entry)
    }
}
