use super::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub struct CacheRoot<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    root: DirCache<N>,
}

impl<N> CacheRoot<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    pub fn new(inner: N) -> Self {
        CacheRoot {
            root: DirCache::new(inner.root()),
        }
    }
}

impl<N> NodeType for CacheRoot<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    type Error = N::Error;
    type File = N::File;
    type Directory = DirCache<N>;
    type Symlink = N::Symlink;

    fn root(&self) -> Self::Directory {
        self.root.clone()
    }
}

#[derive(Clone)]
pub struct DirCache<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    inner: N::Directory,
    cached_files: RefCell<Option<Vec<(String, Node2<CacheRoot<N>>)>>>,
    hidden_cached_files: RefCell<HashMap<String, Node2<CacheRoot<N>>>>,
    non_files: RefCell<HashSet<String>>,
}

impl<N> DirCache<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    pub fn new(inner: N::Directory) -> Self {
        DirCache {
            inner,
            cached_files: RefCell::new(None),
            hidden_cached_files: RefCell::new(HashMap::new()),
            non_files: RefCell::new(HashSet::new()),
        }
    }
}

impl<N> Meta for DirCache<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    type Error = N::Error;
    fn metadata(&self) -> Result<Metadata, Self::Error> {
        self.inner.metadata()
    }
}

impl<N> Directory<CacheRoot<N>> for DirCache<N>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    fn files(&self) -> Result<Vec<(String, Node2<CacheRoot<N>>)>, Self::Error> {
        let mut cached = self.cached_files.borrow_mut();
        if cached.is_some() {
            return Ok(cached.as_ref().unwrap().to_vec());
        }
        let files: Vec<_> = self
            .inner
            .files()?
            .into_iter()
            .map(|(name, node)| (name, map_node(node)))
            .collect();
        *cached = Some(files.clone());
        Ok(files)
    }

    fn file_by_name(&self, name: &str) -> Result<Node2<CacheRoot<N>>, Self::Error> {
        if self.non_files.borrow().contains(name) {
            return Err(Self::Error::not_found());
        }

        if let Some(node) = self.hidden_cached_files.borrow().get(name) {
            return Ok(node.clone());
        }

        let cached = self.cached_files.borrow_mut();
        if cached.is_some() {
            let maybe_node = cached
                .as_ref()
                .unwrap()
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, entry)| entry);
            if let Some(node) = maybe_node {
                return Ok(node.clone());
            }
        }

        match self.inner.file_by_name(name) {
            Ok(node) => {
                let node = map_node(node);
                self.hidden_cached_files
                    .borrow_mut()
                    .insert(name.to_string(), node.clone());
                Ok(node)
            }
            Err(err) => {
                if err.errno() == libc::ENOENT {
                    self.non_files.borrow_mut().insert(name.to_string());
                }
                Err(err)
            }
        }
    }
}

fn map_node<N>(node: Node2<N>) -> Node2<CacheRoot<N>>
where
    N: NodeType + Clone,
    N::File: Clone,
    N::Directory: Clone,
    N::Symlink: Clone,
{
    match node {
        Node2::File(f) => Node2::File(f),
        Node2::Directory(f) => Node2::Directory(DirCache::new(f)),
        Node2::Symlink(f) => Node2::Symlink(f),
    }
}
