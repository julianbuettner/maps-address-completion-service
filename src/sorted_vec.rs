use std::{ops::Deref, collections::HashSet};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SortedVec<T>(Vec<T>);

impl<T: Ord> SortedVec<T> {
    // pub fn new() -> Self {
    //     Self(Vec::new())
    // }
    // pub fn insert(&mut self, e: T) {
    //     let i = match self.0.binary_search(&e) {
    //         Err(i) => i,
    //         Ok(i) => i,
    //     };
    //     self.0.insert(i, e);
    // }
    // pub fn insert_if_not_containing(&mut self, e: T) {
    //     match self.0.binary_search(&e) {
    //         Err(i) => self.0.insert(i, e),
    //         Ok(_) => (),
    //     }
    // }
    pub fn index_of(&self, e: &T) -> Option<usize> {
        self.0.binary_search(e).ok()
    }
    // pub fn contains(&self, e: &T) -> bool {
    //     self.0.binary_search(e).is_ok()
    // }
}

impl<T> Deref for SortedVec<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Ord> From<Vec<T>> for SortedVec<T> {
    fn from(mut value: Vec<T>) -> Self {
        value.sort();
        Self(value)
    }
}

impl<T: Ord> From<HashSet<T>> for SortedVec<T> {
    fn from(value: HashSet<T>) -> Self {
        let vec: Vec<T> = value.into_iter().collect();
        vec.into()
    }
}
