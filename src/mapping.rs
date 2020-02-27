use crate::filesystem;
use crate::id3tag::tag_for_track;
use crate::ioutil::{Concat, LazyOpen, ReadSeek, Skip};
use crate::mp3;
use crate::soundcloud;
use chrono::Utc;
use id3;
use std::error;
use std::fmt;
use std::io::{self, Seek};
use std::path::PathBuf;

const PADDING_START: u64 = 500;
const PADDING_END: u64 = 20;

#[derive(Debug)]
pub enum Error {
    ChildNotFound,

    SoundCloudError(soundcloud::Error),
    IOError(io::Error),
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

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for Error {}

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

// TODO: Use proper lifetimes to share state and make this private.
#[derive(Clone)]
pub struct RootState {
    pub sc_client: soundcloud::Client,
    pub show: Vec<String>,
    pub mpeg_padding: bool,
    pub id3_download_images: bool,
    pub id3_parse_strings: bool,
}

#[derive(Clone)]
pub struct Root<'a> {
    inner: &'a RootState,
}

impl<'a> Root<'a> {
    pub fn new(inner: &'a RootState) -> Self {
        Root { inner }
    }
}

impl<'a> filesystem::NodeType for Root<'a> {
    type Error = Error;
    type File = TrackAudio<'a>;
    type Directory = Dir<'a>;
    type Symlink = UserReference;

    fn root(&self) -> Self::Directory {
        Dir::UserList(UserList { inner: &self.inner })
    }
}

// Clippy complains about a large size difference in the enum variants. This is because the
// UserList is the only variant that does not has a user field. The warning has been silenced
// because the UserList variant will only be instantiated once.
#[allow(clippy::large_enum_variant)]
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
    fn files(&self) -> Result<Vec<(String, filesystem::Node<Root<'a>>)>, Self::Error> {
        match self {
            Dir::UserList(f) => f.files(),
            Dir::UserProfile(f) => f.files(),
            Dir::UserFavorites(f) => f.files(),
            Dir::UserFollowing(f) => f.files(),
        }
    }

    fn file_by_name(&self, name: &str) -> Result<filesystem::Node<Root<'a>>, Self::Error> {
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
    inner: &'a RootState,
}

impl filesystem::Meta for UserList<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        let now = Utc::now();
        Ok(filesystem::Metadata {
            mtime: now,
            ctime: now,
            perm: 0o555,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserList<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node<Root<'a>>)>, Self::Error> {
        self.inner
            .show
            .iter()
            .map(|name| {
                let entry = filesystem::Node::Directory(Dir::UserProfile(UserProfile {
                    inner: &self.inner,
                    user: soundcloud::User::by_name(&self.inner.sc_client, name)?,
                    recurse: true,
                }));
                Ok((name.clone(), entry))
            })
            .collect()
    }

    fn file_by_name(&self, name: &str) -> Result<filesystem::Node<Root<'a>>, Self::Error> {
        match name {
            "autorun.inf" | "BDMV" => {
                return Err(Error::ChildNotFound);
            }
            name if name.starts_with('.') => {
                return Err(Error::ChildNotFound);
            }
            _ => (),
        }
        let entry = filesystem::Node::Directory(Dir::UserProfile(UserProfile {
            inner: &self.inner,
            user: soundcloud::User::by_name(&self.inner.sc_client, name)?,
            recurse: self.inner.show.iter().any(|n| n == name),
        }));
        Ok(entry)
    }
}

#[derive(Clone)]
pub struct UserFavorites<'a> {
    inner: &'a RootState,
    user: soundcloud::User,
}

impl filesystem::Meta for UserFavorites<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o555,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserFavorites<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node<Root<'a>>)>, Self::Error> {
        let files: Vec<_> = self
            .user
            .favorites(&self.inner.sc_client)?
            .into_iter()
            .map(|track| {
                (
                    format!("{}_-_{}.mp3", track.user.permalink, track.permalink),
                    filesystem::Node::File(TrackAudio {
                        inner: self.inner,
                        track,
                    }),
                )
            })
            .collect();
        Ok(files)
    }
}

#[derive(Clone)]
pub struct UserFollowing<'a> {
    inner: &'a RootState,
    user: soundcloud::User,
}

impl filesystem::Meta for UserFollowing<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o555,
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserFollowing<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node<Root<'a>>)>, Self::Error> {
        let files: Vec<_> = self
            .user
            .following(&self.inner.sc_client)?
            .into_iter()
            .map(|user| {
                (
                    user.permalink.clone(),
                    filesystem::Node::Symlink(UserReference { user }),
                )
            })
            .collect();
        Ok(files)
    }
}

#[derive(Clone)]
pub struct UserProfile<'a> {
    inner: &'a RootState,
    user: soundcloud::User,
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
        })
    }
}

impl<'a> filesystem::Directory<Root<'a>> for UserProfile<'a> {
    fn files(&self) -> Result<Vec<(String, filesystem::Node<Root<'a>>)>, Self::Error> {
        let mut files = Vec::new();
        if self.recurse {
            files.push((
                "favorites".to_string(),
                filesystem::Node::Directory(Dir::UserFavorites(UserFavorites {
                    user: self.user.clone(),
                    inner: self.inner,
                })),
            ));
            files.push((
                "following".to_string(),
                filesystem::Node::Directory(Dir::UserFollowing(UserFollowing {
                    inner: self.inner,
                    user: self.user.clone(),
                })),
            ));
        }
        let tracks = self
            .user
            .tracks(&self.inner.sc_client)?
            .into_iter()
            .map(|track| {
                (
                    format!("{}.mp3", track.permalink),
                    filesystem::Node::File(TrackAudio {
                        inner: self.inner,
                        track,
                    }),
                )
            });
        files.extend(tracks);
        Ok(files)
    }
}

#[derive(Clone)]
pub struct TrackAudio<'a> {
    inner: &'a RootState,
    track: soundcloud::Track,
}

impl filesystem::Meta for TrackAudio<'_> {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.track.last_modified,
            ctime: self.track.last_modified,
            perm: 0o444,
        })
    }
}

impl<'a> filesystem::File for TrackAudio<'a> {
    type Reader = Concat<Box<dyn ReadSeek + 'a>>;

    fn open_ro(&self) -> Result<Self::Reader, Self::Error> {
        let id3_tag = tag_for_track(
            &self.track,
            self.inner.id3_download_images,
            self.inner.id3_parse_strings,
        )?;

        let remote_mp3_size = self.track.audio_size() as u64;
        let padding_len = mp3::ZERO_FRAME.len() as u64;
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
        let padding_start = mp3::zero_frames(PADDING_START);
        // We also need some padding at the end for players that try to
        // read ID3v1 metadata.
        let padding_end = mp3::zero_frames(PADDING_END);

        let track_cp = self.track.clone();
        let sc_client_cp = &self.inner.sc_client;
        let audio = LazyOpen::with_size_hint(remote_mp3_size, move || {
            let f = track_cp
                .audio(sc_client_cp)
                .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{}", err)))?;
            Ok(Skip::new(f, first_frame_size))
        });

        let concat = if self.inner.mpeg_padding {
            Concat::new(vec![
                Box::<dyn ReadSeek>::from(Box::new(id3_tag)),
                Box::<dyn ReadSeek>::from(Box::new(io::Cursor::new(mp3_header))),
                Box::<dyn ReadSeek>::from(Box::new(padding_start)),
                Box::<dyn ReadSeek>::from(Box::new(audio)),
                Box::<dyn ReadSeek>::from(Box::new(padding_end)),
            ])
        } else {
            Concat::new(vec![
                Box::<dyn ReadSeek>::from(Box::new(id3_tag)),
                Box::<dyn ReadSeek>::from(Box::new(audio)),
            ])
        };
        Ok(concat)
    }

    fn size(&self) -> Result<u64, Self::Error> {
        let id3_tag_size = {
            let mut b = tag_for_track(
                &self.track,
                self.inner.id3_download_images,
                self.inner.id3_parse_strings,
            )?;
            b.seek(io::SeekFrom::End(0)).unwrap()
        };
        let padding_size = if self.inner.mpeg_padding {
            let padding_len = mp3::ZERO_FRAME.len() as u64;
            PADDING_START * padding_len + PADDING_END * padding_len
        } else {
            0
        };
        Ok(id3_tag_size + padding_size + self.track.audio_size() as u64)
    }
}

#[derive(Clone)]
pub struct UserReference {
    user: soundcloud::User,
}

impl filesystem::Meta for UserReference {
    type Error = Error;
    fn metadata(&self) -> Result<filesystem::Metadata, Self::Error> {
        Ok(filesystem::Metadata {
            mtime: self.user.last_modified,
            ctime: self.user.last_modified,
            perm: 0o444,
        })
    }
}

impl filesystem::Symlink for UserReference {
    fn read_link(&self) -> Result<PathBuf, Self::Error> {
        Ok(["..", "..", &self.user.permalink].iter().collect())
    }
}
