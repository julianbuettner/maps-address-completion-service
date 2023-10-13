use std::ops::Deref;


pub struct SortedVec<T> (Vec<T>);

impl<T: Ord> SortedVec<T> {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn insert(&mut self, e: T) {
        let i = match self.0.binary_search(&e) {
            Err(i) => i,
            Ok(i) => i,
        };
        self.0.insert(i, e);
    }
    pub fn contains(&self, e: &T) -> bool {
        self.0.binary_search(e).is_ok()
    }
}

impl<T> Deref for SortedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
