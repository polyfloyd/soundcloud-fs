use super::*;
use failure::Fail;
use ioutil::*;
use std::path::PathBuf;

pub trait Error: Fail {
    fn not_found() -> Self;
    fn errno(&self) -> i32;
}

pub trait Node<'a>: Sized {
    type Error: Error;

    fn file_attributes(&self, ino: u64) -> fuse::FileAttr;

    fn open_ro(&self) -> Result<Box<ReadSeek + 'a>, Self::Error>;

    fn children(&self) -> Result<Vec<(String, Self)>, Self::Error>;

    fn child_by_name(&self, name: &str) -> Result<Self, Self::Error>;

    fn read_link(&self) -> Result<PathBuf, Self::Error>;
}
