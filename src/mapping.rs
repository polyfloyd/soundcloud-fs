use chrono::{DateTime, Utc};
use filesystem;
use id3;
use ioutil;
use soundcloud;
use std::io;
use time;

const BLOCK_SIZE: u64 = 1024;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "soundcloud error: {}", _0)]
    SoundCloudError(soundcloud::Error),

    #[fail(display = "io error: {}", _0)]
    IOError(io::Error),

    #[fail(display = "id3 error: {}", _0)]
    ID3Error(id3::Error),
}

impl filesystem::Error for Error {
    fn errno(&self) -> i32 {
        match self {
            Error::SoundCloudError(_) => libc::EIO,
            Error::IOError(err) => err.raw_os_error().unwrap_or(libc::EIO),
            Error::ID3Error(_) => libc::EIO,
        }
    }
}

impl From<soundcloud::Error> for Error {
    fn from(err: soundcloud::Error) -> Error {
        Error::SoundCloudError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IOError(err)
    }
}

impl From<id3::Error> for Error {
    fn from(err: id3::Error) -> Error {
        Error::ID3Error(err)
    }
}

#[derive(Clone, Debug)]
pub enum Entry<'a> {
    User(soundcloud::User<'a>),
    UserFavorites(soundcloud::User<'a>),
    UserFollowing(soundcloud::User<'a>),
    Track(soundcloud::Track<'a>),
}

impl<'a> filesystem::Node<'a> for Entry<'a> {
    type Error = Error;

    fn file_attributes(&self, ino: u64) -> fuse::FileAttr {
        match self {
            Entry::User(user) => {
                let mtime = timespec_from_datetime(&user.last_modified);
                fuse::FileAttr {
                    ino,
                    size: 0,
                    blocks: 1,
                    atime: mtime,
                    mtime,
                    ctime: mtime,
                    crtime: mtime,
                    kind: fuse::FileType::Directory,
                    perm: 0o555,
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 1,
                    flags: 0,
                }
            }
            Entry::UserFavorites(user) => {
                let mtime = timespec_from_datetime(&user.last_modified);
                fuse::FileAttr {
                    ino,
                    size: 0,
                    blocks: 1,
                    atime: mtime,
                    mtime,
                    ctime: mtime,
                    crtime: mtime,
                    kind: fuse::FileType::Directory,
                    perm: 0o555,
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 1,
                    flags: 0,
                }
            }
            Entry::UserFollowing(user) => {
                let mtime = timespec_from_datetime(&user.last_modified);
                fuse::FileAttr {
                    ino,
                    size: 0,
                    blocks: 1,
                    atime: mtime,
                    mtime,
                    ctime: mtime,
                    crtime: mtime,
                    kind: fuse::FileType::Directory,
                    perm: 0o555,
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 1,
                    flags: 0,
                }
            }
            Entry::Track(track) => {
                let ctime = timespec_from_datetime(&track.created_at);
                let mtime = timespec_from_datetime(&track.last_modified);
                fuse::FileAttr {
                    ino,
                    size: track.original_content_size,
                    blocks: track.original_content_size / BLOCK_SIZE + 1,
                    atime: mtime,
                    mtime,
                    ctime,
                    crtime: ctime,
                    kind: fuse::FileType::RegularFile,
                    perm: 0o444,
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 1,
                    flags: 0,
                }
            }
        }
    }

    fn open_ro(&self) -> Result<Box<ioutil::ReadSeek + 'a>, Error> {
        match self {
            Entry::Track(track) => {
                let mut audio = Box::new(track.audio()?);

                let mut id3_tag_buf = Vec::new();
                let id3_tag = track.id3_tag()?;
                id3_tag.write_to(&mut id3_tag_buf, id3::Version::Id3v24)?;
                let id3_tag_cursor = Box::new(io::Cursor::new(id3_tag_buf));

                let concat = ioutil::Concat::new(vec![
                    Box::<ioutil::ReadSeek>::from(id3_tag_cursor),
                    Box::<ioutil::ReadSeek>::from(audio),
                ])?;
                Ok(Box::new(concat))
            }
            _ => unreachable!("only tracks can be opened for reading"),
        }
    }

    fn children(&self) -> Result<Vec<(String, Entry<'a>)>, Error> {
        match self {
            Entry::User(user) => {
                let mut children = Vec::new();
                children.push(("favorites".to_string(), Entry::UserFavorites(user.clone())));
                if user.primary_email_confirmed.is_some() {
                    // Only add the following directory for the logged in user to prevent recursing
                    // too deeply.
                    children.push(("following".to_string(), Entry::UserFollowing(user.clone())));
                }
                children.extend(
                    user.tracks()?
                        .into_iter()
                        .map(|track| map_track_to_child(track)),
                );
                Ok(children)
            }
            Entry::UserFavorites(user) => {
                let children: Vec<_> = user
                    .favorites()?
                    .into_iter()
                    .map(|track| map_track_to_child(track))
                    .collect();
                Ok(children)
            }
            Entry::UserFollowing(user) => {
                let children: Vec<_> = user
                    .following()?
                    .into_iter()
                    .map(|user| (user.permalink.clone(), Entry::User(user)))
                    .collect();
                Ok(children)
            }
            Entry::Track(_) => unreachable!("tracks do not have child files"),
        }
    }
}

fn map_track_to_child(track: soundcloud::Track) -> (String, Entry) {
    let name = {
        let title = track
            .title
            .replace(|c: char| !c.is_alphanumeric() && !c.is_whitespace(), "")
            .replace("  ", " ")
            .replace(|c: char| c.is_whitespace(), "_");
        let ext = track.audio_format();
        format!("{}_{}.{}", title, track.id, ext)
    };
    (name, Entry::Track(track))
}

fn timespec_from_datetime(t: &DateTime<Utc>) -> time::Timespec {
    time::Timespec::new(t.timestamp(), t.timestamp_subsec_nanos() as i32)
}
