mod node;
mod nodecache;

use chrono::{DateTime, Utc};
use fuse;
use log::*;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::ffi;
use std::hash::{Hash, Hasher};
use std::io::{self, Read, Seek};
use std::os;
use std::os::unix::ffi::OsStrExt;

pub use self::node::*;
pub use self::node::{Metadata, NodeType};
pub use self::nodecache::*;
pub use crate::ioutil::*;

const INO_ROOT: u64 = 1;

pub struct FS<N>
where
    N: NodeType,
{
    nodes: HashMap<u64, Node<N>>,

    read_handles: HashMap<u64, <N::File as File>::Reader>,
    next_read_handle: u64,

    readdir_handles: HashMap<u64, Vec<(String, Node<N>, u64)>>,
    next_readdir_handle: u64,

    uid: u32,
    gid: u32,
}

impl<'a, N> FS<N>
where
    N: NodeType,
{
    pub fn new(root: &N, uid: u32, gid: u32) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(INO_ROOT, Node::Directory(root.root()));
        FS {
            nodes,
            read_handles: HashMap::new(),
            next_read_handle: 1,
            readdir_handles: HashMap::new(),
            next_readdir_handle: 1,
            uid,
            gid,
        }
    }
}

impl<N> fuse::Filesystem for FS<N>
where
    N: NodeType,
{
    fn init(&mut self, _req: &fuse::Request) -> Result<(), os::raw::c_int> {
        trace!("fuse init");
        Ok(())
    }

    fn destroy(&mut self, _req: &fuse::Request) {
        trace!("fuse destroy");
    }

    fn lookup(
        &mut self,
        _req: &fuse::Request,
        parent_ino: u64,
        os_name: &ffi::OsStr,
        reply: fuse::ReplyEntry,
    ) {
        let name = os_name.to_string_lossy();
        trace!("fuse lookup, {}, {}", parent_ino, name);

        let child = {
            let parent = match self.nodes.get(&parent_ino) {
                Some(v) => v,
                None => {
                    error!("fuse: no node for inode {}", parent_ino);
                    reply.error(libc::ENOENT);
                    return;
                }
            };
            let dir = parent
                .directory()
                .expect("can not call lookup on a non-directory");
            match dir.file_by_name(&name) {
                Ok(v) => v,
                Err(err) => {
                    if err.errno() != libc::ENOENT {
                        error!("fuse: could not get child {}: {}", name, err);
                    }
                    reply.error(err.errno());
                    return;
                }
            }
        };

        let child_ino = inode_for_child(parent_ino, &name);

        let attrs = match attrs_for_file(&child, child_ino, self.uid, self.gid) {
            Ok(v) => v,
            Err(err) => {
                error!("fuse: can not get attrs for {}: {}", child_ino, err);
                reply.error(err.errno());
                return;
            }
        };

        self.nodes.insert(child_ino, child);

        let now = time::now().to_timespec();
        reply.entry(&now, &attrs, 0);
    }

    fn getattr(&mut self, _req: &fuse::Request, ino: u64, reply: fuse::ReplyAttr) {
        trace!("fuse getattr: {}", ino);

        if let Some(node) = self.nodes.get(&ino) {
            let attrs = match attrs_for_file(&node, ino, self.uid, self.gid) {
                Ok(v) => v,
                Err(err) => {
                    error!("fuse: can not get attrs for {}: {}", ino, err);
                    reply.error(err.errno());
                    return;
                }
            };
            let ttl = (time::now() + time::Duration::seconds(30)).to_timespec();
            reply.attr(&ttl, &attrs);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn readlink(&mut self, _req: &fuse::Request, ino: u64, reply: fuse::ReplyData) {
        trace!("fuse readlink: ino={}", ino);

        if let Some(node) = self.nodes.get(&ino) {
            let symlink = node
                .symlink()
                .expect("can not call readlink on a non-symlink");
            let path = match symlink.read_link() {
                Ok(v) => v,
                Err(err) => {
                    error!("fuse: could not read symlink: {}", err);
                    reply.error(err.errno());
                    return;
                }
            };
            reply.data(path.as_os_str().as_bytes());
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn open(&mut self, _req: &fuse::Request, ino: u64, flags: u32, reply: fuse::ReplyOpen) {
        trace!("fuse open: {}, {:b}", ino, flags);

        const WRITE_FLAGS: i32 = libc::O_APPEND | libc::O_CREAT | libc::O_EXCL | libc::O_TRUNC;
        if flags & WRITE_FLAGS as u32 != 0 {
            error!("fuse: encountered write flag {:b}", flags);
            reply.error(libc::EROFS);
            return;
        }

        let node = match self.nodes.get(&ino) {
            Some(v) => v,
            None => {
                error!("fuse: no such inode: {}", ino);
                reply.error(libc::ENOENT);
                return;
            }
        };
        let file = node.file().expect("can not call open on a non-file");
        let reader = match file.open_ro() {
            Ok(v) => v,
            Err(err) => {
                error!("fuse: could not read inode {}: {}", ino, err);
                reply.error(libc::EIO);
                return;
            }
        };

        let fh = self.next_read_handle;
        self.next_read_handle += 1;
        self.read_handles.insert(fh, reader);
        reply.opened(fh, flags);
    }

    fn read(
        &mut self,
        _req: &fuse::Request,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        reply: fuse::ReplyData,
    ) {
        trace!(
            "fuse read: ino={}, fh={}, offset={}, size={}",
            ino,
            fh,
            offset,
            size
        );

        let reader = match self.read_handles.get_mut(&fh) {
            Some(e) => e,
            None => {
                error!("fuse: no such open read handle, {}, inode {}", fh, ino);
                reply.error(libc::EBADF);
                return;
            }
        };

        if let Err(err) = reader.seek(io::SeekFrom::Start(offset as u64)) {
            error!("fuse: {}", err);
            reply.error(libc::EIO);
            return;
        }
        trace!("seek to {} ok", offset);
        let mut buf = vec![0; size as usize];
        let nread = match reader.read(&mut buf[..]) {
            Ok(v) => v,
            Err(err) => {
                error!("fuse: {}", err);
                reply.error(libc::EIO);
                return;
            }
        };
        trace!("read {} bytes ok", nread);
        reply.data(&buf[..nread]);
    }

    fn release(
        &mut self,
        _req: &fuse::Request,
        ino: u64,
        fh: u64,
        flags: u32,
        lock_owner: u64,
        flush: bool,
        reply: fuse::ReplyEmpty,
    ) {
        trace!(
            "fuse release: {}, {}, {}, {}, {}",
            ino,
            fh,
            flags,
            lock_owner,
            flush
        );

        self.read_handles.remove(&fh);
        reply.ok();
    }

    fn opendir(
        &mut self,
        _req: &fuse::Request,
        parent_ino: u64,
        flags: u32,
        reply: fuse::ReplyOpen,
    ) {
        trace!("fuse opendir: {}, {}", parent_ino, flags);

        let children = {
            let node = match self.nodes.get(&parent_ino) {
                Some(v) => v,
                None => {
                    error!("fuse: no entry for inode {}", parent_ino);
                    reply.error(libc::ENOENT);
                    return;
                }
            };
            let dir = node
                .directory()
                .expect("can not call lookup on a non-directory");
            match dir.files() {
                Ok(v) => v,
                Err(err) => {
                    error!(
                        "fuse: could not get children for inode {}: {}",
                        parent_ino, err
                    );
                    reply.error(libc::EIO);
                    return;
                }
            }
        };
        let entries = children
            .into_iter()
            .map(|(name, entry)| {
                let ino = inode_for_child(parent_ino, &name);
                (name, entry, ino)
            })
            .collect();

        let fh = self.next_readdir_handle;
        self.next_readdir_handle += 1;
        self.readdir_handles.insert(fh, entries);
        reply.opened(fh, flags);
    }

    fn readdir(
        &mut self,
        _req: &fuse::Request,
        parent_ino: u64,
        fh: u64,
        offset: i64,
        mut reply: fuse::ReplyDirectory,
    ) {
        trace!("fuse readdir: {}, {}, {}", parent_ino, fh, offset);

        let entries = match self.readdir_handles.get(&fh) {
            Some(e) => e,
            None => {
                error!(
                    "fuse: no open readdir handle for handle {}, inode {}",
                    fh, parent_ino
                );
                reply.error(libc::EBADF);
                return;
            }
        };

        let iter = entries.iter().skip(offset as usize).enumerate();
        for (i, (name, node, ino)) in iter {
            let typ = filetype_for_node(&node);
            trace!("fuse readdir node: {} {:?}, {}", ino, typ, name);
            if reply.add(*ino, offset + i as i64 + 1, typ, name) {
                break;
            }
        }
        reply.ok();
    }

    fn releasedir(
        &mut self,
        _req: &fuse::Request,
        parent_ino: u64,
        fh: u64,
        flags: u32,
        reply: fuse::ReplyEmpty,
    ) {
        trace!("fuse releasedir: {}, {}, {}", parent_ino, fh, flags);

        self.readdir_handles.remove(&fh);
        reply.ok();
    }

    fn access(&mut self, _req: &fuse::Request, ino: u64, mask: u32, reply: fuse::ReplyEmpty) {
        trace!("fuse access: {}, {}", ino, mask);
        reply.ok();
    }

    //    fn getlk(
    //        &mut self,
    //        _req: &fuse::Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _lock_owner: u64,
    //        _start: u64,
    //        _end: u64,
    //        _typ: u32,
    //        _pid: u32,
    //        _reply: fuse::ReplyLock,
    //    ) {
    //        unimplemented!();
    //    }
    //    fn setlk(
    //        &mut self,
    //        _req: &fuse::Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _lock_owner: u64,
    //        _start: u64,
    //        _end: u64,
    //        _typ: u32,
    //        _pid: u32,
    //        _sleep: bool,
    //        _reply: fuse::ReplyEmpty,
    //    ) {
    //        unimplemented!();
    //    }
    //    fn statfs(&mut self, _req: &fuse::Request, ino: u64, _reply: fuse::ReplyStatfs) {
    //    }
    //    fn getxattr(
    //        &mut self,
    //        _req: &fuse::Request,
    //        _ino: u64,
    //        _os_name: &ffi::OsStr,
    //        _size: u32,
    //        reply: fuse::ReplyXattr,
    //    ) {
    //        unimplemented!();
    //    }
    //
    //    fn listxattr(&mut self, _req: &fuse::Request, _ino: u64, _size: u32, _reply: fuse::ReplyXattr) {
    //        unimplemented!();
    //    }
    //    fn forget(&mut self, _req: &Request, _ino: u64, _nlookup: u64) { ... }
    //    fn setattr(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _mode: Option<u32>,
    //        _uid: Option<u32>,
    //        _gid: Option<u32>,
    //        _size: Option<u64>,
    //        _atime: Option<Timespec>,
    //        _mtime: Option<Timespec>,
    //        _fh: Option<u64>,
    //        _crtime: Option<Timespec>,
    //        _chgtime: Option<Timespec>,
    //        _bkuptime: Option<Timespec>,
    //        _flags: Option<u32>,
    //        reply: ReplyAttr
    //    ) { ... }
    //    fn mknod(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        _mode: u32,
    //        _rdev: u32,
    //        reply: ReplyEntry
    //    ) { ... }
    //    fn mkdir(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        _mode: u32,
    //        reply: ReplyEntry
    //    ) { ... }
    //    fn unlink(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn rmdir(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn symlink(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        _link: &Path,
    //        reply: ReplyEntry
    //    ) { ... }
    //    fn rename(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        _newparent: u64,
    //        _newname: &OsStr,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn link(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _newparent: u64,
    //        _newname: &OsStr,
    //        reply: ReplyEntry
    //    ) { ... }
    //    fn write(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _offset: i64,
    //        _data: &[u8],
    //        _flags: u32,
    //        reply: ReplyWrite
    //    ) { ... }
    //    fn flush(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _lock_owner: u64,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn fsync(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _datasync: bool,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn fsyncdir(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _fh: u64,
    //        _datasync: bool,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn setxattr(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _name: &OsStr,
    //        _value: &[u8],
    //        _flags: u32,
    //        _position: u32,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn removexattr(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _name: &OsStr,
    //        reply: ReplyEmpty
    //    ) { ... }
    //    fn create(
    //        &mut self,
    //        _req: &Request,
    //        _parent: u64,
    //        _name: &OsStr,
    //        _mode: u32,
    //        _flags: u32,
    //        reply: ReplyCreate
    //    ) { }
    //    fn bmap(
    //        &mut self,
    //        _req: &Request,
    //        _ino: u64,
    //        _blocksize: u32,
    //        _idx: u64,
    //        reply: ReplyBmap
    //    ) { }
}

fn inode_for_child(parent_ino: u64, name: &str) -> u64 {
    let mut s = DefaultHasher::new();
    parent_ino.hash(&mut s);
    name.hash(&mut s);
    s.finish()
}

fn filetype_for_node<N: NodeType>(node: &Node<N>) -> fuse::FileType {
    match node {
        Node::File(_) => fuse::FileType::RegularFile,
        Node::Directory(_) => fuse::FileType::Directory,
        Node::Symlink(_) => fuse::FileType::Symlink,
    }
}

fn timespec_from_datetime(t: &DateTime<Utc>) -> time::Timespec {
    time::Timespec::new(t.timestamp(), t.timestamp_subsec_nanos() as i32)
}

fn attrs_for_file<N: NodeType>(
    node: &Node<N>,
    ino: u64,
    uid: u32,
    gid: u32,
) -> Result<fuse::FileAttr, N::Error> {
    const BLOCK_SIZE: u64 = 1024;

    let meta = node.metadata()?;
    let size = match node {
        Node::File(f) => f.size()?,
        _ => 0,
    };
    Ok(fuse::FileAttr {
        ino,
        size,
        blocks: size / BLOCK_SIZE,
        atime: timespec_from_datetime(&meta.mtime),
        mtime: timespec_from_datetime(&meta.mtime),
        ctime: timespec_from_datetime(&meta.ctime),
        crtime: timespec_from_datetime(&meta.ctime),
        kind: filetype_for_node(&node),
        perm: meta.perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        flags: 0,
    })
}
