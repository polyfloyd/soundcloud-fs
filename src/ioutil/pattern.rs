use std::io;

pub struct Pattern<T: AsRef<[u8]>> {
    pat: T,
    size: u64,
    offset: u64,
}

impl<T: AsRef<[u8]>> Pattern<T> {
    pub fn new(pat: T, size: u64) -> Self {
        Pattern {
            pat,
            size,
            offset: 0,
        }
    }
}

impl<T: AsRef<[u8]>> io::Read for Pattern<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let pat_len = self.pat.as_ref().len();
        if pat_len == 0 {
            return Ok(0);
        }

        let buf_len = (self.size - self.offset).min(buf.len() as u64) as usize;
        let mut sub_buf = &mut buf[..buf_len];

        while !sub_buf.is_empty() {
            let pat_start = (self.offset % pat_len as u64) as usize;
            let pat_end = (pat_start + sub_buf.len()).min(pat_len);
            let buf_end = sub_buf.len().min(pat_end - pat_start);

            sub_buf[..buf_end].copy_from_slice(&self.pat.as_ref()[pat_start..pat_end]);
            sub_buf = &mut sub_buf[buf_end..];
            self.offset += buf_end as u64;
        }

        Ok(buf_len)
    }
}

impl<T: AsRef<[u8]>> io::Seek for Pattern<T> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset as i64,
            io::SeekFrom::Current(offset) => self.offset as i64 + offset,
            io::SeekFrom::End(offset) => self.size as i64 + offset,
        };
        if new_offset < 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "ioutil::Pattern: seek position {:?} resolves to {}",
                    pos, new_offset
                ),
            ));
        }
        self.offset = new_offset as u64;
        Ok(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn read_zero_sized() {
        let mut pat = Pattern::new([], 16);
        let mut buf = [0; 16];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(0, nread);
    }

    #[test]
    fn read_partial() {
        let mut pat = Pattern::new([1, 2, 3, 4, 5, 6, 7, 8], 16);
        let mut buf = [0; 4];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, [1, 2, 3, 4]);
    }

    #[test]
    fn read_once_exact() {
        let mut pat = Pattern::new([1, 2, 3, 4, 5, 6, 7, 8], 8);
        let mut buf = [0; 8];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn read_once_large_buf() {
        let mut pat = Pattern::new([1, 2, 3, 4, 5, 6, 7, 8], 8);
        let mut buf = [0; 16];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn read_multi_exact() {
        let mut pat = Pattern::new([1, 2, 3, 4], 8);
        let mut buf = [0; 8];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(nread, 8);
        assert_eq!(buf, [1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn read_multi_partial() {
        let mut pat = Pattern::new([1, 2, 3, 4], 32);
        let mut buf = [0; 10];
        let nread = pat.read(&mut buf[..]).unwrap();
        assert_eq!(nread, 10);
        assert_eq!(buf, [1, 2, 3, 4, 1, 2, 3, 4, 1, 2]);
    }
}
