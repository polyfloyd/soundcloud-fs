use std::io;

pub trait ReadSeek: io::Read + io::Seek {}

impl<T> ReadSeek for T where T: io::Read + io::Seek {}
