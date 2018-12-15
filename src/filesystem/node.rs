use super::*;
use chrono::{DateTime, Utc};
use failure::Fail;
use std::iter::Iterator;
use std::path::PathBuf;

//pub trait Node<'a>: Sized {
//    type Error: Error;
//
//    fn file_attributes(&self, ino: u64) -> fuse::FileAttr;
//
//    fn open_ro(&self) -> Result<Box<ReadSeek + 'a>, Self::Error>;
//
//    fn children(&self) -> Result<Vec<(String, Self)>, Self::Error>;
//
//    fn child_by_name(&self, name: &str) -> Result<Self, Self::Error>;
//
//    fn read_link(&self) -> Result<PathBuf, Self::Error>;
//}

pub trait Error: Fail {
    fn not_found() -> Self;
    fn errno(&self) -> i32;
}

#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub mtime: DateTime<Utc>,
    pub ctime: DateTime<Utc>,
    pub perm: u16,
    pub uid: u32,
    pub gid: u32,
}

pub trait Meta {
    type Error: Error;
    fn metadata(&self) -> Result<Metadata, Self::Error>;
}

pub trait File: Meta {
    type Reader: io::Read + io::Seek;
    fn open_ro(&self) -> Result<Self::Reader, Self::Error>;
    fn size(&self) -> Result<u64, Self::Error>;
}

pub trait Directory<N: NodeType + ?Sized>: Meta {
    fn files(&self) -> Result<Vec<(String, Node2<N>)>, Self::Error>;

    fn file_by_name(&self, name: &str) -> Result<Node2<N>, Self::Error> {
        self.files()?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, entry)| entry)
            .ok_or_else(|| Self::Error::not_found())
    }
}

pub trait Symlink: Meta {
    fn read_link(&self) -> Result<PathBuf, Self::Error>;
}

pub trait NodeType {
    type Error: Error;
    type File: File<Error = Self::Error>;
    type Directory: Directory<Self, Error = Self::Error>;
    type Symlink: Symlink<Error = Self::Error>;

    fn root(&self) -> Self::Directory;
}

pub enum Node2<T: NodeType + ?Sized> {
    File(T::File),
    Directory(T::Directory),
    Symlink(T::Symlink),
}

impl<T: NodeType> Node2<T> {
    pub fn file(&self) -> Option<&T::File> {
        match self {
            Node2::File(ref f) => Some(f),
            _ => None,
        }
    }

    pub fn directory(&self) -> Option<&T::Directory> {
        match self {
            Node2::Directory(ref f) => Some(f),
            _ => None,
        }
    }

    pub fn symlink(&self) -> Option<&T::Symlink> {
        match self {
            Node2::Symlink(ref f) => Some(f),
            _ => None,
        }
    }
}

impl<T: NodeType> Meta for Node2<T> {
    type Error = T::Error;
    fn metadata(&self) -> Result<Metadata, Self::Error> {
        match self {
            Node2::File(f) => f.metadata(),
            Node2::Directory(f) => f.metadata(),
            Node2::Symlink(f) => f.metadata(),
        }
    }
}
