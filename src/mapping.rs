use crate::filesystem;
use crate::ioutil::{Concat, LazyOpen, ReadSeek, Skip};
use crate::mp3;
use chrono::Utc;
use id3;
use soundcloud;
use std::io::{self, Seek};
use std::path::PathBuf;

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

#[derive(Clone)]
pub struct Root<'a> {
    pub sc_client: &'a soundcloud::Client,
    pub username: String,
}

impl<'a> filesystem::NodeType for Root<'a> {
    type Error = Error;
    type File = TrackAudio<'a>;
    type Directory = Dir<'a>;
    type Symlink = UserReference<'a>;

    fn root(&self) -> Self::Directory {
        let root = UserList::new(&self.sc_client, vec![self.username.clone()]);
        Dir::UserList(root)
    }
}

#[derive(Clone)]
pub enum Dir<'a> {
    UserList(UserList<'a>),
    UserProfile(UserProfile<'a>),
    UserFavorites(UserFavorites<'a>),
    UserFollowing(UserFollowing<'a>),
}

impl filesystem::Meta for Dir<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        match self {
            Dir::UserList(f) => f.metadata(),
            Dir::UserProfile(f) => f.metadata(),
            Dir::UserFavorites(f) => f.metadata(),
            Dir::UserFollowing(f) => f.metadata(),
        }
    }
}

impl<'a> filesystem::Directory<Root<'a>> for Dir<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node2<Root<'a>>)>, Self::Error> {
        match self {
            Dir::UserList(f) => f.files(),
            Dir::UserProfile(f) => f.files(),
            Dir::UserFavorites(f) => f.files(),
            Dir::UserFollowing(f) => f.files(),
        }
    }

    fn file_by_name(&self, name: &str) -> Result<filesystem::Node2<Root<'a>>, Self::Error> {
        match self {
            Dir::UserList(f) => f.file_by_name(name),
            Dir::UserProfile(f) => f.file_by_name(name),
            Dir::UserFavorites(f) => f.file_by_name(name),
            Dir::UserFollowing(f) => f.file_by_name(name),
        }
    }
}

#[derive(Clone)]
pub struct UserList<'a> {
    sc_client: &'a soundcloud::Client,
    show: Vec<String>,
}

impl<'a> UserList<'a> {
    pub fn new(sc_client: &'a soundcloud::Client, show: Vec<String>) -> Self {
        UserList { sc_client, show }
    }
}

impl filesystem::Meta for UserList<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        let now = Utc::now();
        Ok(filesystem::Metadata {
            mtime: now,
            ctime: now,
            perm: 0o555,
            uid: 1,
            gid: 1,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserList<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node2<Root<'a>>)>, Self::Error> {
        self.show
            .iter()
            .map(|name| {
                let entry = filesystem::Node2::Directory(Dir::UserProfile(UserProfile {
                    user: soundcloud::User::by_name(&self.sc_client, name)?,
                    recurse: true,
                }));
                Ok((name.clone(), entry))
            })
            .collect()
    }

    fn file_by_name(&self, name: &str) -> Result<filesystem::Node2<Root<'a>>, Self::Error> {
        match name {
            "autorun.inf" | "BDMV" => {
                return Err(Error::ChildNotFound);
            }
            name if name.starts_with('.') => {
                return Err(Error::ChildNotFound);
            }
            _ => (),
        }
        let entry = filesystem::Node2::Directory(Dir::UserProfile(UserProfile {
            user: soundcloud::User::by_name(&self.sc_client, name)?,
            recurse: self.show.iter().any(|n| n == name),
        }));
        Ok(entry)
    }
}

#[derive(Clone)]
pub struct UserFavorites<'a> {
    user: soundcloud::User<'a>,
}

impl filesystem::Meta for UserFavorites<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o555,
            uid: 1,
            gid: 1,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserFavorites<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node2<Root<'a>>)>, Self::Error> {
        let files: Vec<_> = self
            .user
            .favorites()?
            .into_iter()
            .map(|track| {
                (
                    format!("{}_-_{}.mp3", track.user.permalink, track.permalink),
                    filesystem::Node2::File(TrackAudio { track }),
                )
            })
            .collect();
        Ok(files)
    }
}

#[derive(Clone)]
pub struct UserFollowing<'a> {
    user: soundcloud::User<'a>,
}

impl filesystem::Meta for UserFollowing<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o555,
            uid: 1,
            gid: 1,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserFollowing<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node2<Root<'a>>)>, Self::Error> {
        let files: Vec<_> = self
            .user
            .following()?
            .into_iter()
            .map(|user| {
                (
                    user.permalink.clone(),
                    filesystem::Node2::Symlink(UserReference { user }),
                )
            })
            .collect();
        Ok(files)
    }
}

#[derive(Clone)]
pub struct UserProfile<'a> {
    user: soundcloud::User<'a>,
    // Only add child directories for users marked for recursing, to prevent recursing too deeply.
    recurse: bool,
}

impl filesystem::Meta for UserProfile<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o555,
            uid: 1,
            gid: 1,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserProfile<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node2<Root<'a>>)>, Self::Error> {
        let mut files = Vec::new();
        if self.recurse {
            files.push((
                "favorites".to_string(),
                filesystem::Node2::Directory(Dir::UserFavorites(UserFavorites {
                    user: self.user.clone(),
                })),
            ));
            files.push((
                "following".to_string(),
                filesystem::Node2::Directory(Dir::UserFollowing(UserFollowing {
                    user: self.user.clone(),
                })),
            ));
        }
        let tracks = self.user.tracks()?.into_iter().map(|track| {
            (
                format!("{}.mp3", track.permalink),
                filesystem::Node2::File(TrackAudio { track }),
            )
        });
        files.extend(tracks);
        Ok(files)
    }
}

#[derive(Clone)]
pub struct TrackAudio<'a> {
    track: soundcloud::Track<'a>,
}

impl filesystem::Meta for TrackAudio<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.track.last_modified,
            ctime: self.track.last_modified,
            perm: 0o444,
            uid: 1,
            gid: 1,
        })
    }
}

impl<'a> filesystem::File for TrackAudio<'a> {
    type Reader = Concat<Box<ReadSeek + 'a>>;

    fn open_ro(&self) -> Result<Self::Reader, Self::Error> {
        let id3_tag = self.track.id3_tag()?;

        let remote_mp3_size = self.track.audio_size() as u64;
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

        let track_cp = self.track.clone();
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
        Ok(concat)
    }

    fn size(&self) -> Result<u64, Self::Error> {
        let id3_tag_size = {
            let mut b = self.track.id3_tag()?;
            b.seek(io::SeekFrom::End(0)).unwrap()
        };
        let mp3_size = {
            let padding_len = mp3::zero_headers(1).len() as u64;
            self.track.audio_size() as u64 + PADDING_START * padding_len + PADDING_END * padding_len
        };
        Ok(id3_tag_size + mp3_size)
    }
}

#[derive(Clone)]
pub struct UserReference<'a> {
    user: soundcloud::User<'a>,
}

impl filesystem::Meta for UserReference<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o444,
            uid: 1,
            gid: 1,
        })
    }
}

impl filesystem::Symlink for UserReference<'_> {
    fn read_link(&self) -> Result<PathBuf, Self::Error> {
        Ok(["..", "..", &self.user.permalink].iter().collect())
    }
}
