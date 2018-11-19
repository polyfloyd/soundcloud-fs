use super::*;
use ioutil::*;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct NodeCache<'a, T>
where
    T: Node<'a> + Clone,
{
    inner: T,
    cached_children: RefCell<Option<Vec<(String, NodeCache<'a, T>)>>>,
}

impl<'a, T> NodeCache<'a, T>
where
    T: Node<'a> + Clone,
{
    pub fn new(inner: T) -> NodeCache<'a, T> {
        NodeCache {
            inner,
            cached_children: RefCell::new(None),
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
}
