use std::cmp::Ordering;
use std::io;
use std::ops::Range;

/// Concat is a collection of files that are concatenated after one another as if they were a
/// single file.
///
/// It is assumed that the underlying files do not change in size.
pub struct Concat<T>
where
    T: io::Read,
{
    files: Vec<T>,
    chunk_index: usize,
    offset: u64,

    ranges: Vec<Range<u64>>,
}

impl<T> Concat<T>
where
    T: io::Read,
{
    pub fn new(files: Vec<T>) -> io::Result<Concat<T>> {
        if files.is_empty() {
            return Err(io::Error::new(io::ErrorKind::Other, "no files specified"));
        }

        Ok(Concat {
            files,
            ranges: Vec::new(),
            chunk_index: 0,
            offset: 0,
        })
    }
}

impl<T> io::Read for Concat<T>
where
    T: io::Read + io::Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let num_files = self.files.len();
        let mut total_nread = 0;
        let mut next_chunk = false;
        while total_nread < buf.len() && self.chunk_index < num_files {
            let file = &mut self.files[self.chunk_index];
            if next_chunk {
                // If we transition into the next chunk, ensure that it is set to the beginning.
                file.seek(io::SeekFrom::Start(0))?;
                next_chunk = false;
            }

            let nread = file.read(&mut buf[total_nread..])?;
            if nread == 0 {
                self.chunk_index += 1;
                next_chunk = true;
                if self.chunk_index >= num_files {
                    break;
                }
                continue;
            }
            total_nread += nread;
            self.offset += nread as u64;
        }

        Ok(total_nread)
    }
}

impl<T> io::Seek for Concat<T>
where
    T: io::Read + io::Seek,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        if self.ranges.is_empty() {
            let mut ranges = Vec::new();
            let mut offset = 0;
            for mut file in &mut self.files {
                let size = file.seek(io::SeekFrom::End(0))?;
                let range = offset..(offset + size);
                ranges.push(range);
                offset += size;
            }
            self.ranges = ranges;
        }

        let length = self.ranges.last().unwrap().end as i64;
        let new_offset: i64 = match pos {
            io::SeekFrom::Start(offset) => offset as i64,
            io::SeekFrom::End(offset) => length + offset,
            io::SeekFrom::Current(offset) => self.offset as i64 + offset,
        };

        if new_offset < 0 || length < new_offset {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "ioutil::Concat: seek position {:?} outside range 0..{}",
                    pos, length
                ),
            ));
        }

        self.offset = new_offset as u64;
        self.chunk_index = self
            .ranges
            .binary_search_by(|range| {
                if self.offset < range.start {
                    Ordering::Greater
                } else if self.offset >= range.end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }).unwrap_or_else(|_| self.ranges.len() - 1);

        let file = &mut self.files[self.chunk_index];
        let range = &mut self.ranges[self.chunk_index];
        file.seek(io::SeekFrom::Start(new_offset as u64 - range.start))?;
        Ok(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek};

    #[test]
    fn read_single_file() {
        let expect = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut concat = Concat::new(vec![io::Cursor::new(expect.clone())]).unwrap();

        let mut buf = vec![0; expect.len()];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, expect.len());
        assert_eq!(buf, expect);

        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 0);
    }

    #[test]
    fn read_single_file_multi() {
        let expect = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut concat = Concat::new(vec![io::Cursor::new(expect.clone())]).unwrap();

        let mut buf = vec![0; 4];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, vec![1, 2, 3, 4]);

        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, vec![5, 6, 7, 8]);

        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 0);
    }

    #[test]
    fn read_multiple_files() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        let expect = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut buf = vec![0; expect.len()];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(expect.len(), nread);
        assert_eq!(expect, buf);
    }

    #[test]
    fn read_multiple_files_multiread() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        let mut buf = vec![0; 6];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 6);
        assert_eq!(buf, vec![1, 2, 3, 4, 5, 6]);

        let mut buf = vec![0; 4];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![7, 8, 0, 0]);

        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 0);
    }

    #[test]
    fn seek_single_file() {
        let expect = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut concat = Concat::new(vec![io::Cursor::new(expect.clone())]).unwrap();

        concat.seek(io::SeekFrom::Start(4)).unwrap();

        let mut buf = vec![0; 4];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 4);
        assert_eq!(buf, vec![5, 6, 7, 8]);
    }

    #[test]
    fn seek_multiple_files_eof() {
        let expect = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let mut concat = Concat::new(vec![io::Cursor::new(expect.clone())]).unwrap();

        concat.seek(io::SeekFrom::End(0)).unwrap();

        let mut buf = vec![0; 4];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 0);
    }

    #[test]
    fn seek_multiple_files_a() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        concat.seek(io::SeekFrom::Start(2)).unwrap();

        let mut buf = vec![0; 6];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 6);
        assert_eq!(buf, vec![3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn seek_multiple_files_b() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        concat.seek(io::SeekFrom::Start(6)).unwrap();

        let mut buf = vec![0; 2];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![7, 8]);
    }

    #[test]
    fn seek_after_read() {
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        let mut buf = vec![0; 2];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![1, 2]);

        concat.seek(io::SeekFrom::Start(6)).unwrap();

        let mut buf = vec![0; 2];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![7, 8]);

        concat.seek(io::SeekFrom::Start(2)).unwrap();

        let mut buf = vec![0; 2];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 2);
        assert_eq!(buf, vec![3, 4]);
    }
}
