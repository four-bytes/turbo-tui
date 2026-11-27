//! Inline link model: [`Link`], [`LinkTarget`], [`LinkId`] and [`LinkTable`].
//!
//! A link describes a target that rendered text can point at (a URL, a source
//! position, an issue, or a command). The cells of a [`Buffer`](crate::core::Buffer)
//! carry an optional [`LinkId`], and the buffer owns a [`LinkTable`] that maps
//! those ids back to the full [`Link`].

use crate::command::CommandId;

/// Identifier of an entry in a [`LinkTable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(u32);

impl LinkId {
    /// Returns the numeric id.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Where a link points.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkTarget {
    /// An external URL.
    Url(String),
    /// A reference to an issue in a repository.
    IssueRef {
        /// Repository (e.g. `owner/repo`), when known.
        repo: Option<String>,
        /// Issue number.
        number: u32,
    },
    /// A position inside a file.
    FilePos {
        /// Path of the file.
        path: String,
        /// One-based line number.
        line: u32,
        /// Optional one-based column number.
        col: Option<u32>,
    },
    /// An application command, with an optional payload.
    Command(CommandId, Option<String>),
}

/// One linkable target plus an optional human-readable title.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Link {
    /// The link target.
    pub target: LinkTarget,
    /// Optional human-readable title.
    pub title: Option<String>,
}

impl Link {
    /// Creates a link for `target` without a title.
    #[must_use]
    pub fn new(target: LinkTarget) -> Self {
        Self {
            target,
            title: None,
        }
    }

    /// Sets the human-readable title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Stable id table for the links drawn into one buffer.
///
/// Inserting an equal [`Link`] (same target and title) returns the existing
/// [`LinkId`], so a repeated link only occupies a single table entry.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct LinkTable {
    links: Vec<Link>,
}

impl LinkTable {
    /// Creates an empty link table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns an existing id for an equal link, otherwise appends and returns
    /// a new id.
    ///
    /// # Panics
    ///
    /// Panics if the table would need to exceed [`u32::MAX`] entries, which is
    /// beyond any realistic terminal buffer.
    pub fn insert(&mut self, link: &Link) -> LinkId {
        if let Some(index) = self.links.iter().position(|existing| existing == link) {
            return LinkId(u32::try_from(index).expect("link table exceeds u32::MAX"));
        }
        let index = self.links.len();
        self.links.push(link.clone());
        LinkId(u32::try_from(index).expect("link table exceeds u32::MAX"))
    }

    /// Returns the link for `id`, if it exists.
    #[must_use]
    pub fn get(&self, id: LinkId) -> Option<&Link> {
        self.links.get(id.get() as usize)
    }

    /// Returns the number of entries in the table.
    #[must_use]
    pub fn len(&self) -> usize {
        self.links.len()
    }

    /// Returns `true` when the table holds no links.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.links.is_empty()
    }

    /// Removes every entry from the table.
    pub fn clear(&mut self) {
        self.links.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url_link(url: &str) -> Link {
        Link::new(LinkTarget::Url(url.to_owned()))
    }

    #[test]
    fn test_link_table_insert_same_link_returns_same_id() {
        let mut table = LinkTable::new();
        let link = url_link("https://example.com");
        let first = table.insert(&link);
        let second = table.insert(&link);
        assert_eq!(first, second);
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn test_link_table_different_target_or_title_returns_different_id() {
        let mut table = LinkTable::new();
        let a = table.insert(&url_link("https://example.com/a"));
        let b = table.insert(&url_link("https://example.com/b"));
        let c = table.insert(&url_link("https://example.com/a").with_title("A"));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn test_link_table_get_resolves_link() {
        let mut table = LinkTable::new();
        let link = url_link("https://example.com").with_title("Example");
        let id = table.insert(&link);
        assert_eq!(table.get(id), Some(&link));
    }

    #[test]
    fn test_link_table_clear_empties_table() {
        let mut table = LinkTable::new();
        table.insert(&url_link("https://example.com"));
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }
}
