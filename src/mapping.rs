use chrono::{DateTime, Utc};
use filesystem;
use id3;
use ioutil::{Concat, LazyOpen, ReadSeek, Skip};
use mp3;
use soundcloud;
use std::io::{self, Seek};
use std::path::PathBuf;
use time;

const BLOCK_SIZE: u64 = 1024;

const PADDING_START: u64 = 500;
const PADDING_END: u64 = 20;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "child not found")]
    ChildNotFound,

    #[fail(display = "soundcloud error: {}", _0)]
    SoundCloudError(soundcloud::Error),

    #[fail(display = "io error: {}", _0)]
    IOError(io::Error),

    #[fail(display = "id3 error: {}", _0)]
    ID3Error(id3::Error),
}

impl filesystem::Error for Error {
    fn not_found() -> Self {
        Error::ChildNotFound
    }

    fn errno(&self) -> i32 {
        match self {
            Error::ChildNotFound => libc::ENOENT,
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
    Users {
        sc_client: &'a soundcloud::Client,
        show: Vec<String>,
    },
    User {
        user: soundcloud::User<'a>,
        // Only add child directories for users marked for recursing, to prevent recursing too
        // deeply.
        recurse: bool,
    },
    UserFavorites(soundcloud::User<'a>),
    UserFollowing(soundcloud::User<'a>),
    UserReference(soundcloud::User<'a>),
    Track(soundcloud::Track<'a>),
}

impl<'a> filesystem::Node<'a> for Entry<'a> {
    type Error = Error;

    fn file_attributes(&self, ino: u64) -> fuse::FileAttr {
        match self {
            Entry::Users { .. } => {
                let mtime = timespec_from_datetime(&Utc::now());
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
            Entry::User { user, .. } => {
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
            Entry::UserReference(user) => {
                let mtime = timespec_from_datetime(&user.last_modified);
                fuse::FileAttr {
                    ino,
                    size: 0,
                    blocks: 1,
                    atime: mtime,
                    mtime,
                    ctime: mtime,
                    crtime: mtime,
                    kind: fuse::FileType::Symlink,
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

                let size = {
                    let id3_tag_size = {
                        let mut b = track.id3_tag().unwrap(); // TODO: remove unwrap
                        b.seek(io::SeekFrom::End(0)).unwrap()
                    };
                    let mp3_size = {
                        let padding_len = mp3::zero_headers(1).len() as u64;
                        track.audio_size() as u64
                            + PADDING_START * padding_len
                            + PADDING_END * padding_len
                    };
                    id3_tag_size + mp3_size
                };

                fuse::FileAttr {
                    ino,
                    size,
                    blocks: size / BLOCK_SIZE + 1,
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

    fn open_ro(&self) -> Result<Box<ReadSeek + 'a>, Error> {
        match self {
            Entry::Track(track) => {
                let id3_tag = track.id3_tag()?;

                let remote_mp3_size = track.audio_size() as u64;
                let padding_len = mp3::zero_headers(1).len() as u64;
                let mp3_total_size =
                    remote_mp3_size + PADDING_START * padding_len + PADDING_END * padding_len;
                let mp3_header = mp3::cbr_header(mp3_total_size);
                let first_frame_size = mp3_header.len() as u64;

                // Hackety hack: the file concatenation abstraction is able to lazily index the
                // size of the underlying files. This ensures for programs that just want to probe
                // the audio file's metadata, no request for the actual audio file will be
                // performed.
                // However, because reading programs may read beyond the metadata, the audio may
                // still be accessed. To counter this, we jam a very large swath of zero bytes in
                // between the metadata and audio stream to saturate the read buffer without the
                // audio stream.
                let padding_start = mp3::zero_headers(PADDING_START);
                // We also need some padding at the end for players that try to
                // read ID3v1 metadata.
                let padding_end = mp3::zero_headers(PADDING_END);

                let track_cp = track.clone();
                let audio = LazyOpen::with_size_hint(remote_mp3_size, move || {
                    let f = track_cp
                        .audio()
                        .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))?;
                    Ok(Skip::new(f, first_frame_size))
                });

                let concat = Concat::new(vec![
                    Box::<ReadSeek>::from(Box::new(id3_tag)),
                    Box::<ReadSeek>::from(Box::new(io::Cursor::new(mp3_header))),
                    Box::<ReadSeek>::from(Box::new(io::Cursor::new(padding_start))),
                    Box::<ReadSeek>::from(Box::new(audio)),
                    Box::<ReadSeek>::from(Box::new(io::Cursor::new(padding_end))),
                ])?;
                Ok(Box::new(concat))
            }
            _ => unreachable!("only tracks can be opened for reading"),
        }
    }

    fn children(&self) -> Result<Vec<(String, Entry<'a>)>, Error> {
        match self {
            Entry::Users { sc_client, show } => show
                .iter()
                .map(|name| {
                    let entry = Entry::User {
                        user: soundcloud::User::by_name(&sc_client, name)?,
                        recurse: true,
                    };
                    Ok((name.clone(), entry))
                })
                .collect(),
            Entry::User { user, recurse } => {
                let mut children = Vec::new();
                if *recurse {
                    children.push(("favorites".to_string(), Entry::UserFavorites(user.clone())));
                    children.push(("following".to_string(), Entry::UserFollowing(user.clone())));
                }
                children.extend(user.tracks()?.into_iter().map(map_track_to_child));
                Ok(children)
            }
            Entry::UserFavorites(user) => {
                let children: Vec<_> = user
                    .favorites()?
                    .into_iter()
                    .map(map_track_to_child)
                    .collect();
                Ok(children)
            }
            Entry::UserFollowing(user) => {
                let children: Vec<_> = user
                    .following()?
                    .into_iter()
                    .map(|user| (user.permalink.clone(), Entry::UserReference(user)))
                    .collect();
                Ok(children)
            }
            Entry::UserReference(_) => unreachable!("user referebces do not have child files"),
            Entry::Track(_) => unreachable!("tracks do not have child files"),
        }
    }

    fn child_by_name(&self, name: &str) -> Result<Entry<'a>, Error> {
        match self {
            Entry::Users { sc_client, show } => {
                match name {
                    "autorun.inf" | "BDMV" => {
                        return Err(Error::ChildNotFound);
                    }
                    name if name.starts_with('.') => {
                        return Err(Error::ChildNotFound);
                    }
                    _ => (),
                }
                let entry = Entry::User {
                    user: soundcloud::User::by_name(&sc_client, name)?,
                    recurse: show.iter().any(|n| n == name),
                };
                Ok(entry)
            }
            _ => self
                .children()?
                .into_iter()
                .find(|(n, _)| n == name)
                .map(|(_, entry)| entry)
                .ok_or(Error::ChildNotFound),
        }
    }

    fn read_link(&self) -> Result<PathBuf, Error> {
        match self {
            Entry::UserReference(user) => Ok(["..", "..", &user.permalink].iter().collect()),
            _ => unreachable!("not a symlink"),
        }
    }
}

fn map_track_to_child(track: soundcloud::Track) -> (String, Entry) {
    (
        format!("{}_-_{}.mp3", track.user.permalink, track.permalink),
        Entry::Track(track),
    )
}

fn timespec_from_datetime(t: &DateTime<Utc>) -> time::Timespec {
    time::Timespec::new(t.timestamp(), t.timestamp_subsec_nanos() as i32)
}
