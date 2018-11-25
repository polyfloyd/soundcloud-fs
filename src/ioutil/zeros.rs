use std::io;

pub fn zeros(length: u64) -> impl io::Read + io::Seek {
    Zeros {
        length: length as i64,
        offset: 0,
    }
}

struct Zeros {
    length: i64,
    offset: i64,
}

impl io::Read for Zeros {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let limit = (self.length - self.offset).max(0).min(buf.len() as i64);
        let buf = &mut buf[0..limit as usize];
        for b in buf.iter_mut() {
            *b = 0;
        }
        self.offset += limit as i64;
        Ok(buf.len())
    }
}

impl io::Seek for Zeros {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_offset = match pos {
            io::SeekFrom::Start(offset) => offset as i64,
            io::SeekFrom::Current(offset) => self.offset as i64 + offset,
            io::SeekFrom::End(offset) => self.length as i64 + offset,
        };
        if new_offset < 0 {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "ioutil::zeros: seek position {:?} resolves to {}",
                    pos, new_offset,
                ),
            ));
        }
        self.offset = new_offset;
        Ok(self.offset as u64)
    }
}
