use std::io;

pub struct Skip<T>
where
    T: io::Read + io::Seek,
{
    inner: T,
    offset: u64,

    initial_skip: bool,
}

impl<T> Skip<T>
where
    T: io::Read + io::Seek,
{
    pub fn new(inner: T, offset: u64) -> Self {
        Skip {
            inner,
            offset,
            initial_skip: false,
        }
    }
}

impl<T> io::Read for Skip<T>
where
    T: io::Read + io::Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.initial_skip {
            self.inner.seek(io::SeekFrom::Start(self.offset))?;
            self.initial_skip = true;
        }
        self.inner.read(buf)
    }
}

impl<T> io::Seek for Skip<T>
where
    T: io::Read + io::Seek,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        // TODO: prevent seeking before self.offset.
        let new_pos = match pos {
            io::SeekFrom::Start(offset) => io::SeekFrom::Start(self.offset + offset),
            io::SeekFrom::End(offset) => io::SeekFrom::End(offset),
            io::SeekFrom::Current(offset) => io::SeekFrom::Current(offset),
        };
        Ok(self.inner.seek(new_pos)? - self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn skip() {
        let file: Vec<u8> = (0..16).collect();
        let mut skip = Skip::new(io::Cursor::new(file), 8);

        let mut buf = [0; 16];
        let nread = skip.read(&mut buf).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(
            &buf,
            &[8, 9, 10, 11, 12, 13, 14, 15, 0, 0, 0, 0, 0, 0, 0, 0]
        );
    }
}
