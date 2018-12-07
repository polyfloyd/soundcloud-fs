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

impl<T> Concat<T>
where
    T: io::Read + io::Seek,
{
    /// index_up_to ensures that there is a range in `self.ranges` that includes the specified
    /// offset unless there are no more files to index.
    fn index_up_to(&mut self, new_offset: u64) -> io::Result<()> {
        loop {
            let current_end = self.ranges.last().map(|r| r.end).unwrap_or(0);
            if (new_offset as u64) < current_end {
                break Ok(());
            }

            let file = match self.files.get_mut(self.ranges.len()) {
                Some(v) => v,
                None => break Ok(()),
            };
            let size = file.seek(io::SeekFrom::End(0))?;
            let range = current_end..(current_end + size);
            self.ranges.push(range);
        }
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
        let new_offset: i64 = match pos {
            io::SeekFrom::Start(offset) => {
                self.index_up_to(offset)?;
                offset as i64
            }
            io::SeekFrom::Current(offset) => {
                let new_offset = self.offset as i64 + offset;
                if new_offset < 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "ioutil::Concat: seek position {:?} resolves to {}",
                            pos, new_offset
                        ),
                    ));
                }
                self.index_up_to(new_offset as u64)?;
                new_offset
            }
            io::SeekFrom::End(offset) => {
                // Before we can know where the end is, we have to index all files. We do this by
                // just trying to index up the closest we can get to infinity with a 64 bit int.
                self.index_up_to(std::u64::MAX)?;
                let length = self.ranges.last().unwrap().end as i64;
                let new_offset = length + offset;
                if new_offset < 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "ioutil::Concat: seek position {:?} resolves to {}",
                            pos, new_offset
                        ),
                    ));
                }
                new_offset
            }
        };

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
            })
            .unwrap_or_else(|_| self.ranges.len() - 1);

        let file = &mut self.files[self.chunk_index];
        let range = &mut self.ranges[self.chunk_index];
        file.seek(io::SeekFrom::Start(new_offset as u64 - range.start))?;
        Ok(self.offset)
    }
}

#[cfg(test)]
mod tests {
    use super::super::OpRecorder;
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
        let a = vec![1, 2, 3, 4];
        let b = vec![5, 6, 7, 8];
        let mut concat =
            Concat::new(vec![io::Cursor::new(a.clone()), io::Cursor::new(b.clone())]).unwrap();

        let abs_pos = concat.seek(io::SeekFrom::End(0)).unwrap();
        assert_eq!(abs_pos, 8);

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

    #[test]
    fn seek_lazy_ranges() {
        let mut concat = Concat::new(vec![
            OpRecorder::new(io::Cursor::new(vec![0; 4])),
            OpRecorder::new(io::Cursor::new(vec![0; 4])),
        ])
        .unwrap();

        let mut buf = vec![0; 4];
        concat.read(&mut buf).unwrap();
        concat.seek(io::SeekFrom::Start(3)).unwrap();

        println!("{:?}", concat.files[1].ops());
        assert_eq!(concat.files[1].ops().len(), 0);
    }

    #[test]
    fn seek_beyond_length() {
        let mut concat = Concat::new(vec![
            io::Cursor::new(vec![0; 4]),
            io::Cursor::new(vec![0; 4]),
        ])
        .unwrap();

        concat.seek(io::SeekFrom::End(8)).unwrap();

        let mut buf = vec![0; 4];
        let nread = concat.read(&mut buf).unwrap();
        assert_eq!(nread, 0);
    }
}
