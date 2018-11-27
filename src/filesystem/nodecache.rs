use super::*;
use ioutil::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct NodeCache<'a, T>
where
    T: Node<'a> + Clone,
{
    inner: T,
    cached_children: RefCell<Option<Vec<(String, NodeCache<'a, T>)>>>,
    hidden_cached_children: RefCell<HashMap<String, NodeCache<'a, T>>>,
    non_children: RefCell<HashSet<String>>,
}

impl<'a, T> NodeCache<'a, T>
where
    T: Node<'a> + Clone,
{
    pub fn new(inner: T) -> NodeCache<'a, T> {
        NodeCache {
            inner,
            cached_children: RefCell::new(None),
            hidden_cached_children: RefCell::new(HashMap::new()),
            non_children: RefCell::new(HashSet::new()),
        }
    }
}

impl<'a, T> Node<'a> for NodeCache<'a, T>
where
    T: Node<'a> + Clone,
{
    type Error = T::Error;

    fn file_attributes(&self, ino: u64) -> fuse::FileAttr {
        self.inner.file_attributes(ino)
    }

    fn open_ro(&self) -> Result<Box<ReadSeek + 'a>, Self::Error> {
        self.inner.open_ro()
    }

    fn children(&self) -> Result<Vec<(String, Self)>, Self::Error> {
        let mut cached = self.cached_children.borrow_mut();
        if cached.is_some() {
            return Ok(cached.as_ref().unwrap().clone());
        }
        let children: Vec<_> = self
            .inner
            .children()?
            .into_iter()
            .map(|(name, node)| (name, NodeCache::new(node)))
            .collect();
        *cached = Some(children.clone());
        Ok(children)
    }

    fn child_by_name(&self, name: &str) -> Result<Self, Self::Error> {
        if self.non_children.borrow().contains(name) {
            return Err(Self::Error::not_found());
        }

        if let Some(child) = self.hidden_cached_children.borrow().get(name) {
            return Ok(child.clone());
        }

        let cached = self.cached_children.borrow_mut();
        if cached.is_some() {
            let maybe_child = cached
                .as_ref()
                .unwrap()
                .iter()
                .find(|(n, _)| n == &name)
                .map(|(_, entry)| entry);
            if let Some(child) = maybe_child {
                return Ok(child.clone());
            }
        }

        match self.inner.child_by_name(name) {
            Ok(v) => {
                let child = NodeCache::new(v);
                self.hidden_cached_children
                    .borrow_mut()
                    .insert(name.to_string(), child.clone());
                Ok(child)
            }
            Err(err) => {
                if err.errno() == libc::ENOENT {
                    self.non_children.borrow_mut().insert(name.to_string());
                }
                Err(err)
            }
        }
    }

    fn read_link(&self) -> Result<PathBuf, Self::Error> {
        self.inner.read_link()
    }
}
