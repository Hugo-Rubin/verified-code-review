//! A tag, and a set of tags.

use std::collections::HashSet;

/// A label attached to a resource.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Tag(String);

impl Tag {
    pub fn new(s: impl Into<String>) -> Self {
        Tag(s.into())
    }

    /// The tag exactly as the user wrote it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A set of tags. Adding a tag that is already present is a no-op.
#[derive(Debug, Default)]
pub struct TagSet {
    inner: HashSet<Tag>,
}

impl TagSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true when the tag was not already present.
    pub fn insert(&mut self, t: Tag) -> bool {
        self.inner.insert(t)
    }

    pub fn contains(&self, t: &Tag) -> bool {
        self.inner.contains(t)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_tags_are_equal() {
        assert_eq!(Tag::new("release"), Tag::new("release"));
    }

    #[test]
    fn different_tags_are_not_equal() {
        assert_ne!(Tag::new("release"), Tag::new("draft"));
    }

    #[test]
    fn as_str_keeps_the_original_spelling() {
        assert_eq!(Tag::new("Release").as_str(), "Release");
    }

    #[test]
    fn a_new_set_is_empty() {
        let s = TagSet::new();
        assert!(s.is_empty());
        assert_eq!(s.len(), 0);
    }

    #[test]
    fn inserting_the_same_tag_twice_is_a_no_op() {
        let mut s = TagSet::new();
        assert!(s.insert(Tag::new("release")));
        assert!(!s.insert(Tag::new("release")));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn an_inserted_tag_is_found() {
        let mut s = TagSet::new();
        s.insert(Tag::new("release"));
        assert!(s.contains(&Tag::new("release")));
        assert!(!s.contains(&Tag::new("draft")));
    }

    #[test]
    fn distinct_tags_are_all_kept() {
        let mut s = TagSet::new();
        s.insert(Tag::new("release"));
        s.insert(Tag::new("draft"));
        assert_eq!(s.len(), 2);
    }
}
