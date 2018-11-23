use std::io;

#[derive(Copy, Clone, Debug)]
pub enum Operation {
    Read { buflen: usize, nread: usize },
    Seek { pos: io::SeekFrom, new_pos: u64 },
}

pub struct OpRecorder<T> {
    inner: T,
    ops: Vec<Operation>,
}

impl<T> OpRecorder<T> {
    pub fn new(inner: T) -> OpRecorder<T> {
        OpRecorder {
            inner,
            ops: Vec::new(),
        }
    }

    pub fn ops(&self) -> &[Operation] {
        &self.ops
    }
}

impl<T> io::Read for OpRecorder<T>
where
    T: io::Read,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let nread = self.inner.read(buf)?;
        self.ops.push(Operation::Read {
            buflen: buf.len(),
            nread,
        });
        Ok(nread)
    }
}

impl<T> io::Seek for OpRecorder<T>
where
    T: io::Read + io::Seek,
{
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        let new_pos = self.inner.seek(pos)?;
        self.ops.push(Operation::Seek { pos, new_pos });
        Ok(new_pos)
    }
}
