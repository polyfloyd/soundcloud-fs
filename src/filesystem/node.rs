use super::*;
use chrono::{DateTime, Utc};
use std::error;
use std::iter::Iterator;
use std::path::PathBuf;

pub trait Error: error::Error {
    fn not_found() -> Self;
    fn errno(&self) -> i32;
}

#[derive(Clone, Copy, Debug)]
pub struct Metadata {
    pub mtime: DateTime<Utc>,
    pub ctime: DateTime<Utc>,
    pub perm: u16,
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

pub trait Directory<N: NodeType>: Meta {
    fn files(&self) -> Result<Vec<(String, Node<N>)>, Self::Error>;

    fn file_by_name(&self, name: &str) -> Result<Node<N>, Self::Error> {
        self.files()?
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, entry)| entry)
            .ok_or_else(Self::Error::not_found)
    }
}

pub trait Symlink: Meta {
    fn read_link(&self) -> Result<PathBuf, Self::Error>;
}

pub trait NodeType: Sized {
    type Error: Error;
    type File: File<Error = Self::Error>;
    type Directory: Directory<Self, Error = Self::Error>;
    type Symlink: Symlink<Error = Self::Error>;

    fn root(&self) -> Self::Directory;
}

#[derive(Clone)]
pub enum Node<T: NodeType> {
    File(T::File),
    Directory(T::Directory),
    Symlink(T::Symlink),
}

impl<T: NodeType> Node<T> {
    pub fn file(&self) -> Option<&T::File> {
        match self {
            Node::File(ref f) => Some(f),
            _ => None,
        }
    }

    pub fn directory(&self) -> Option<&T::Directory> {
        match self {
            Node::Directory(ref f) => Some(f),
            _ => None,
        }
    }

    pub fn symlink(&self) -> Option<&T::Symlink> {
        match self {
            Node::Symlink(ref f) => Some(f),
            _ => None,
        }
    }
}

impl<T: NodeType> Meta for Node<T> {
    type Error = T::Error;
    fn metadata(&self) -> Result<Metadata, Self::Error> {
        match self {
            Node::File(f) => f.metadata(),
            Node::Directory(f) => f.metadata(),
            Node::Symlink(f) => f.metadata(),
        }
    }
}
