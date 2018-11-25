use std::io;
use std::mem;

pub enum LazyOpen<O, T>
where
    O: FnOnce() -> io::Result<T>,
    T: io::Read,
{
    Unopened(O),
    Opened(T),
    Error(io::Error),
    Init,
}

impl<O, T> LazyOpen<O, T>
where
    O: FnOnce() -> io::Result<T>,
    T: io::Read,
{
    pub fn new(open_fn: O) -> LazyOpen<O, T> {
        LazyOpen::Unopened(open_fn)
    }

    fn file_mut(&mut self) -> io::Result<&mut T> {
        match self {
            LazyOpen::Opened(ref mut file) => {
                return Ok(file);
            }
            LazyOpen::Error(err) => {
                return Err(io::Error::new(err.kind(), format!("{}", err)));
            }
            LazyOpen::Unopened(_) => (),
            LazyOpen::Init => unreachable!(),
        }

        let mut init = LazyOpen::Init;
        mem::swap(self, &mut init);
        let open_fn = match init {
            LazyOpen::Unopened(v) => v,
            _ => unreachable!(),
        };

        *self = match open_fn() {
            Ok(file) => LazyOpen::Opened(file),
            Err(err) => LazyOpen::Error(err),
        };
        self.file_mut()
    }
}

impl<O, T> io::Read for LazyOpen<O, T>
where
    O: FnOnce() -> io::Result<T>,
    T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let file = self.file_mut()?;
        file.read(buf)
    }
}

impl<O, T> io::Seek for LazyOpen<O, T>
where
    O: FnOnce() -> io::Result<T>,
    T: io::Read + io::Seek,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let file = self.file_mut()?;
        file.seek(pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};

    #[test]
    fn open_read() {
        let data = vec![1, 2, 3, 4];
        let mut file = LazyOpen::new(|| Ok(&data[..]));

        let mut buf = vec![0; 4];
        let nread = file.read(&mut buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, data);
    }

    #[test]
    fn open_seek() {
        let data = vec![1, 2, 3, 4];
        let mut file = LazyOpen::new(|| Ok(io::Cursor::new(data)));

        let new_pos = file.seek(io::SeekFrom::Start(2)).unwrap();
        assert_eq!(new_pos, 2);

        let mut buf = vec![0; 2];
        let nread = file.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![3, 4]);
    }
}
