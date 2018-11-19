use super::ReadSeek;
use std::cmp::Ordering;
use std::io;
use std::ops::Range;

struct Chunk<T> {
    range: Range<u64>,
    file: T,
}

/// Concat is a collection of files that are concatenated after one another as if they were a
/// single file.
///
/// It is assumed that the underlying files do not change in size.
pub struct Concat<'a> {
    mapping: Vec<Chunk<Box<ReadSeek + 'a>>>,
    offset: u64,
}

impl<'a> Concat<'a> {
    pub fn new(files: Vec<Box<ReadSeek + 'a>>) -> io::Result<Concat> {
        if files.is_empty() {
            return Err(io::Error::new(io::ErrorKind::Other, "no files specified"));
        }

        let mut mapping = Vec::new();
        let mut offset = 0;

        for mut file in files.into_iter() {
            let size = file.seek(io::SeekFrom::End(0))?;
            let range = offset..(offset + size);
            mapping.push(Chunk { range, file });
            offset += size;
        }

        Ok(Concat { mapping, offset: 0 })
    }

    fn total_size(&self) -> u64 {
        // Mapping is never empty.
        self.mapping.last().unwrap().range.end
    }

    fn current_chunk_index(&mut self) -> usize {
        self.mapping
            .binary_search_by(|chunk| {
                if self.offset < chunk.range.start {
                    Ordering::Greater
                } else if self.offset >= chunk.range.end {
                    Ordering::Less
                } else {
                    Ordering::Equal
                }
            }).unwrap()
    }
}

impl<'a> io::Read for Concat<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut total_nread = 0;
        while total_nread < buf.len() {
            if self.offset >= self.total_size() {
                break;
            }

            let index = self.current_chunk_index();
            let mapping_len = self.mapping.len();
            let chunk = &mut self.mapping[index];

            let nread = chunk.file.read(&mut buf[total_nread..])?;
            assert!(nread != 0);
            total_nread += nread;
            self.offset += nread as u64;
            if nread == 0 && index == mapping_len - 1 {
                break;
            }
        }

        Ok(total_nread)
    }
}

impl<'a> io::Seek for Concat<'a> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let length = self.total_size() as i64;
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

        let index = self.current_chunk_index();
        let chunk = &mut self.mapping[index];
        chunk.file.seek(io::SeekFrom::Start(new_offset as u64))?;

        self.offset = new_offset as u64;
        Ok(self.offset)
    }
}
