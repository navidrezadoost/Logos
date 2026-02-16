// SPDX-License-Identifier: MPL-2.0
// logos-desktop/src/tabs.rs — Multi-document tab bar
//
//  Manages a set of open documents as tabs.  Each tab tracks its
//  document ID, title, dirty state, and optional file path.  The
//  `TabBar` provides navigation, reordering, close buttons, and
//  context menus.

use std::fmt;

use uuid::Uuid;

// ── Tab Entry ───────────────────────────────────────────────────

/// A single tab representing an open document.
#[derive(Debug, Clone)]
pub struct Tab {
    /// Unique tab identifier.
    pub id: Uuid,
    /// Document ID (matches the `Document.id` in logos-core).
    pub document_id: Uuid,
    /// Display title (file name or "Untitled").
    pub title: String,
    /// Full file path, if the document has been saved.
    pub file_path: Option<String>,
    /// Whether the document has unsaved changes.
    pub dirty: bool,
    /// Whether this tab is pinned (cannot be closed accidentally).
    pub pinned: bool,
    /// Optional tooltip with full path.
    pub tooltip: Option<String>,
}

impl Tab {
    pub fn new(document_id: Uuid, title: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            document_id,
            title: title.into(),
            file_path: None,
            dirty: false,
            pinned: false,
            tooltip: None,
        }
    }

    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        let p = path.into();
        self.tooltip = Some(p.clone());
        self.file_path = Some(p);
        self
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Display title with a dirty indicator.
    pub fn display_title(&self) -> String {
        if self.dirty {
            format!("● {}", self.title)
        } else {
            self.title.clone()
        }
    }

    pub fn toggle_pin(&mut self) {
        self.pinned = !self.pinned;
    }
}

impl fmt::Display for Tab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_title())
    }
}

// ── Close Behavior ──────────────────────────────────────────────

/// What should happen when a tab close is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Close immediately (no unsaved changes).
    Close,
    /// Prompt "Save changes?" dialog.
    PromptSave,
    /// Cannot close (pinned tab).
    Blocked,
}

// ── Tab Bar ─────────────────────────────────────────────────────

/// Manages the tab bar with multiple open documents.
pub struct TabBar {
    tabs: Vec<Tab>,
    active_index: Option<usize>,
    /// Maximum number of tabs before showing overflow.
    pub max_visible_tabs: usize,
    /// Scroll offset when tabs overflow.
    pub scroll_offset: usize,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active_index: None,
            max_visible_tabs: 12,
            scroll_offset: 0,
        }
    }

    /// Add a new tab and make it active.
    pub fn open(&mut self, tab: Tab) -> usize {
        // Check if document is already open
        if let Some(idx) = self.find_by_document(tab.document_id) {
            self.active_index = Some(idx);
            return idx;
        }
        self.tabs.push(tab);
        let idx = self.tabs.len() - 1;
        self.active_index = Some(idx);
        idx
    }

    /// Close a tab by index.  Returns the close action needed.
    pub fn request_close(&self, index: usize) -> CloseAction {
        match self.tabs.get(index) {
            Some(tab) if tab.pinned => CloseAction::Blocked,
            Some(tab) if tab.dirty => CloseAction::PromptSave,
            Some(_) => CloseAction::Close,
            None => CloseAction::Close,
        }
    }

    /// Actually close a tab.
    /// Returns the closed tab, or None if index is out of range.
    pub fn close(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);

        // Adjust active index
        if self.tabs.is_empty() {
            self.active_index = None;
        } else if let Some(active) = self.active_index {
            if active >= self.tabs.len() {
                self.active_index = Some(self.tabs.len() - 1);
            } else if active > index {
                self.active_index = Some(active - 1);
            }
            // If active == index, keep it (now points to the next tab)
        }

        Some(tab)
    }

    /// Close all tabs except the one at `index`.
    pub fn close_others(&mut self, keep_index: usize) -> Vec<Tab> {
        if keep_index >= self.tabs.len() {
            return Vec::new();
        }
        let kept = self.tabs.remove(keep_index);
        let closed: Vec<Tab> = self.tabs.drain(..).collect();
        self.tabs.push(kept);
        self.active_index = Some(0);
        closed
    }

    /// Close all tabs to the right of `index`.
    pub fn close_to_right(&mut self, index: usize) -> Vec<Tab> {
        if index + 1 >= self.tabs.len() {
            return Vec::new();
        }
        let closed: Vec<Tab> = self.tabs.drain(index + 1..).collect();
        if let Some(active) = self.active_index {
            if active > index {
                self.active_index = Some(index);
            }
        }
        closed
    }

    /// Set the active tab by index.
    pub fn activate(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active_index = Some(index);
            true
        } else {
            false
        }
    }

    /// Switch to the next tab (wraps around).
    pub fn next_tab(&mut self) {
        if let Some(active) = self.active_index {
            if !self.tabs.is_empty() {
                self.active_index = Some((active + 1) % self.tabs.len());
            }
        }
    }

    /// Switch to the previous tab (wraps around).
    pub fn previous_tab(&mut self) {
        if let Some(active) = self.active_index {
            if !self.tabs.is_empty() {
                self.active_index = Some(if active == 0 {
                    self.tabs.len() - 1
                } else {
                    active - 1
                });
            }
        }
    }

    /// Move a tab from one position to another (drag reorder).
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);

        // Adjust active index
        if let Some(active) = self.active_index {
            if active == from {
                self.active_index = Some(to);
            } else if from < active && active <= to {
                self.active_index = Some(active - 1);
            } else if to <= active && active < from {
                self.active_index = Some(active + 1);
            }
        }
        true
    }

    /// Get the currently active tab.
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_index.and_then(|i| self.tabs.get(i))
    }

    /// Get mutable access to the active tab.
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_index.and_then(|i| self.tabs.get_mut(i))
    }

    /// Active tab index.
    pub fn active_index(&self) -> Option<usize> {
        self.active_index
    }

    /// All tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Number of open tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Whether any tab has unsaved changes.
    pub fn has_dirty_tabs(&self) -> bool {
        self.tabs.iter().any(|t| t.dirty)
    }

    /// Count of tabs with unsaved changes.
    pub fn dirty_count(&self) -> usize {
        self.tabs.iter().filter(|t| t.dirty).count()
    }

    /// Find a tab by its document ID.
    pub fn find_by_document(&self, doc_id: Uuid) -> Option<usize> {
        self.tabs.iter().position(|t| t.document_id == doc_id)
    }

    /// Find a tab by its file path.
    pub fn find_by_path(&self, path: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.file_path.as_deref() == Some(path))
    }

    /// Whether the tab bar is overflowing (more tabs than visible limit).
    pub fn is_overflowing(&self) -> bool {
        self.tabs.len() > self.max_visible_tabs
    }

    /// Get the visible tab range accounting for scroll offset.
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        let start = self.scroll_offset;
        let end = (start + self.max_visible_tabs).min(self.tabs.len());
        start..end
    }

    /// Scroll to make a tab visible.
    pub fn scroll_to_tab(&mut self, index: usize) {
        if index < self.scroll_offset {
            self.scroll_offset = index;
        } else if index >= self.scroll_offset + self.max_visible_tabs {
            self.scroll_offset = index.saturating_sub(self.max_visible_tabs - 1);
        }
    }
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tab(title: &str) -> Tab {
        Tab::new(Uuid::new_v4(), title)
    }

    #[test]
    fn test_tab_creation() {
        let tab = make_tab("Untitled-1");
        assert_eq!(tab.title, "Untitled-1");
        assert!(!tab.dirty);
        assert!(!tab.pinned);
        assert!(tab.file_path.is_none());
    }

    #[test]
    fn test_tab_with_path() {
        let tab = make_tab("design.logos").with_path("/home/user/design.logos");
        assert_eq!(tab.file_path, Some("/home/user/design.logos".to_string()));
        assert_eq!(tab.tooltip, Some("/home/user/design.logos".to_string()));
    }

    #[test]
    fn test_tab_dirty_indicator() {
        let mut tab = make_tab("MyDoc");
        assert_eq!(tab.display_title(), "MyDoc");
        tab.mark_dirty();
        assert_eq!(tab.display_title(), "● MyDoc");
        tab.mark_clean();
        assert_eq!(tab.display_title(), "MyDoc");
    }

    #[test]
    fn test_tab_display() {
        let tab = make_tab("Test");
        assert_eq!(tab.to_string(), "Test");
    }

    #[test]
    fn test_tab_pin() {
        let mut tab = make_tab("Test");
        assert!(!tab.pinned);
        tab.toggle_pin();
        assert!(tab.pinned);
        tab.toggle_pin();
        assert!(!tab.pinned);
    }

    #[test]
    fn test_tabbar_open() {
        let mut bar = TabBar::new();
        assert_eq!(bar.tab_count(), 0);
        assert!(bar.active_tab().is_none());

        let idx = bar.open(make_tab("Doc 1"));
        assert_eq!(idx, 0);
        assert_eq!(bar.tab_count(), 1);
        assert_eq!(bar.active_tab().unwrap().title, "Doc 1");
    }

    #[test]
    fn test_tabbar_open_deduplicates() {
        let mut bar = TabBar::new();
        let doc_id = Uuid::new_v4();
        let tab1 = Tab::new(doc_id, "Doc");
        let tab2 = Tab::new(doc_id, "Doc"); // same doc_id

        bar.open(tab1);
        let idx = bar.open(tab2);
        assert_eq!(idx, 0); // should reuse existing tab
        assert_eq!(bar.tab_count(), 1);
    }

    #[test]
    fn test_tabbar_close() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));
        assert_eq!(bar.active_index(), Some(2)); // C is active

        let closed = bar.close(2);
        assert!(closed.is_some());
        assert_eq!(closed.unwrap().title, "C");
        assert_eq!(bar.tab_count(), 2);
        assert_eq!(bar.active_index(), Some(1)); // B is now last
    }

    #[test]
    fn test_tabbar_close_middle() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));
        bar.activate(2); // C is active

        bar.close(1); // close B
        assert_eq!(bar.tab_count(), 2);
        assert_eq!(bar.active_index(), Some(1)); // C shifted left
    }

    #[test]
    fn test_close_action_clean() {
        let bar = TabBar::new();
        // No tabs → Close
        assert_eq!(bar.request_close(0), CloseAction::Close);
    }

    #[test]
    fn test_close_action_dirty() {
        let mut bar = TabBar::new();
        let mut tab = make_tab("Dirty");
        tab.mark_dirty();
        bar.open(tab);
        assert_eq!(bar.request_close(0), CloseAction::PromptSave);
    }

    #[test]
    fn test_close_action_pinned() {
        let mut bar = TabBar::new();
        let mut tab = make_tab("Pinned");
        tab.pinned = true;
        bar.open(tab);
        assert_eq!(bar.request_close(0), CloseAction::Blocked);
    }

    #[test]
    fn test_close_others() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));

        let closed = bar.close_others(1); // keep B
        assert_eq!(closed.len(), 2);
        assert_eq!(bar.tab_count(), 1);
        assert_eq!(bar.active_tab().unwrap().title, "B");
    }

    #[test]
    fn test_close_to_right() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));
        bar.open(make_tab("D"));

        let closed = bar.close_to_right(1); // close C, D
        assert_eq!(closed.len(), 2);
        assert_eq!(bar.tab_count(), 2);
    }

    #[test]
    fn test_next_previous_tab() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));
        bar.activate(0);

        bar.next_tab();
        assert_eq!(bar.active_index(), Some(1));
        bar.next_tab();
        assert_eq!(bar.active_index(), Some(2));
        bar.next_tab();
        assert_eq!(bar.active_index(), Some(0)); // wraps

        bar.previous_tab();
        assert_eq!(bar.active_index(), Some(2)); // wraps back
    }

    #[test]
    fn test_reorder() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        bar.open(make_tab("B"));
        bar.open(make_tab("C"));
        bar.activate(0); // A is active

        bar.reorder(0, 2); // move A to end
        assert_eq!(bar.tabs()[0].title, "B");
        assert_eq!(bar.tabs()[1].title, "C");
        assert_eq!(bar.tabs()[2].title, "A");
        assert_eq!(bar.active_index(), Some(2)); // A moved to 2
    }

    #[test]
    fn test_reorder_invalid() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A"));
        assert!(!bar.reorder(0, 5)); // out of bounds
        assert!(!bar.reorder(0, 0)); // same position
    }

    #[test]
    fn test_find_by_document() {
        let mut bar = TabBar::new();
        let doc_id = Uuid::new_v4();
        bar.open(Tab::new(doc_id, "Target"));
        bar.open(make_tab("Other"));

        assert_eq!(bar.find_by_document(doc_id), Some(0));
        assert!(bar.find_by_document(Uuid::new_v4()).is_none());
    }

    #[test]
    fn test_find_by_path() {
        let mut bar = TabBar::new();
        bar.open(make_tab("A").with_path("/home/a.logos"));
        bar.open(make_tab("B").with_path("/home/b.logos"));

        assert_eq!(bar.find_by_path("/home/b.logos"), Some(1));
        assert!(bar.find_by_path("/nonexistent").is_none());
    }

    #[test]
    fn test_dirty_tracking() {
        let mut bar = TabBar::new();
        bar.open(make_tab("Clean"));
        assert!(!bar.has_dirty_tabs());
        assert_eq!(bar.dirty_count(), 0);

        bar.active_tab_mut().unwrap().mark_dirty();
        assert!(bar.has_dirty_tabs());
        assert_eq!(bar.dirty_count(), 1);
    }

    #[test]
    fn test_overflow_detection() {
        let mut bar = TabBar::new();
        bar.max_visible_tabs = 3;
        for i in 0..5 {
            bar.open(make_tab(&format!("Tab {i}")));
        }
        assert!(bar.is_overflowing());
        assert_eq!(bar.visible_range(), 0..3);
    }

    #[test]
    fn test_scroll_to_tab() {
        let mut bar = TabBar::new();
        bar.max_visible_tabs = 3;
        for i in 0..10 {
            bar.open(make_tab(&format!("Tab {i}")));
        }
        bar.scroll_to_tab(8);
        assert!(bar.scroll_offset > 0);
        assert!(bar.visible_range().contains(&8));
    }

    #[test]
    fn test_close_last_tab() {
        let mut bar = TabBar::new();
        bar.open(make_tab("Only"));
        bar.close(0);
        assert_eq!(bar.tab_count(), 0);
        assert!(bar.active_index().is_none());
        assert!(bar.active_tab().is_none());
    }
}
