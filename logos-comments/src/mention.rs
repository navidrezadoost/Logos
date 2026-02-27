//! @mention parsing and indexing.
//!
//! Extracts `@username` tokens from comment text and maintains an index
//! mapping users to comments that mention them for notification routing.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::model::{CommentId, ThreadId};

// ── Mention ──────────────────────────────────────────────────────────

/// A parsed @mention in a comment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mention {
    /// The username after @.
    pub username: String,
    /// Byte offset of the @ in the original text.
    pub offset: usize,
    /// Length of the full @mention token in bytes.
    pub length: usize,
    /// Resolved user ID (set when the mention is resolved against a user directory).
    pub user_id: Option<Uuid>,
}

impl Mention {
    pub fn new(username: impl Into<String>, offset: usize, length: usize) -> Self {
        Self {
            username: username.into(),
            offset,
            length,
            user_id: None,
        }
    }

    /// Create a resolved mention.
    pub fn resolved(
        username: impl Into<String>,
        offset: usize,
        length: usize,
        user_id: Uuid,
    ) -> Self {
        Self {
            username: username.into(),
            offset,
            length,
            user_id: Some(user_id),
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.user_id.is_some()
    }

    /// The full mention text (e.g., "@alice").
    pub fn mention_text(&self) -> String {
        format!("@{}", self.username)
    }
}

// ── Parser ───────────────────────────────────────────────────────────

/// Parse @mentions from text.
///
/// Recognizes `@username` patterns where username consists of
/// alphanumeric characters, underscores, hyphens, and dots.
///
/// ```
/// use logos_comments::parse_mentions;
///
/// let mentions = parse_mentions("Hey @alice and @bob-smith, check this");
/// assert_eq!(mentions.len(), 2);
/// assert_eq!(mentions[0].username, "alice");
/// assert_eq!(mentions[1].username, "bob-smith");
/// ```
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    let mut mentions = Vec::new();
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut pos = 0;

    while pos < len {
        if bytes[pos] == b'@' {
            let at_pos = pos;
            pos += 1;

            // Parse username: [a-zA-Z0-9_\-.]+ but must start with alpha/underscore
            if pos < len && (bytes[pos].is_ascii_alphabetic() || bytes[pos] == b'_') {
                let start = pos;
                while pos < len
                    && (bytes[pos].is_ascii_alphanumeric()
                        || bytes[pos] == b'_'
                        || bytes[pos] == b'-'
                        || bytes[pos] == b'.')
                {
                    pos += 1;
                }
                // Don't end with a dot or hyphen
                while pos > start && (bytes[pos - 1] == b'.' || bytes[pos - 1] == b'-') {
                    pos -= 1;
                }
                if pos > start {
                    let username = &text[start..pos];
                    mentions.push(Mention::new(username, at_pos, pos - at_pos));
                }
            }
        } else {
            pos += 1;
        }
    }

    mentions
}

/// Resolve mentions against a user directory (name -> Uuid mapping).
pub fn resolve_mentions(
    mentions: &mut [Mention],
    directory: &HashMap<String, Uuid>,
) {
    for m in mentions.iter_mut() {
        if let Some(uid) = directory.get(&m.username) {
            m.user_id = Some(*uid);
        }
        // Also try case-insensitive lookup
        if m.user_id.is_none() {
            let lower = m.username.to_lowercase();
            for (name, uid) in directory {
                if name.to_lowercase() == lower {
                    m.user_id = Some(*uid);
                    break;
                }
            }
        }
    }
}

// ── Mention Index ────────────────────────────────────────────────────

/// An index mapping user IDs to the threads/comments that mention them.
///
/// Used for efficient notification routing — when a comment is added,
/// check which users are mentioned and notify them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MentionIndex {
    /// user_id → Vec<(thread_id, comment_id)>
    index: HashMap<Uuid, Vec<(ThreadId, CommentId)>>,
}

impl MentionIndex {
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Record that a user was mentioned in a specific comment.
    pub fn add_mention(
        &mut self,
        user_id: Uuid,
        thread_id: ThreadId,
        comment_id: CommentId,
    ) {
        let entry = self.index.entry(user_id).or_default();
        let pair = (thread_id, comment_id);
        if !entry.contains(&pair) {
            entry.push(pair);
        }
    }

    /// Remove all mentions for a specific comment (e.g., on delete).
    pub fn remove_comment_mentions(
        &mut self,
        thread_id: ThreadId,
        comment_id: CommentId,
    ) {
        for entries in self.index.values_mut() {
            entries.retain(|&(tid, cid)| !(tid == thread_id && cid == comment_id));
        }
        // Remove empty entries
        self.index.retain(|_, v| !v.is_empty());
    }

    /// Get all (thread, comment) pairs that mention a specific user.
    pub fn mentions_of(&self, user_id: Uuid) -> &[(ThreadId, CommentId)] {
        self.index.get(&user_id).map_or(&[], |v| v.as_slice())
    }

    /// How many times a user has been mentioned.
    pub fn mention_count(&self, user_id: Uuid) -> usize {
        self.index.get(&user_id).map_or(0, |v| v.len())
    }

    /// All users who have been mentioned at least once.
    pub fn mentioned_users(&self) -> Vec<Uuid> {
        self.index.keys().copied().collect()
    }

    /// Clear the entire index.
    pub fn clear(&mut self) {
        self.index.clear();
    }

    /// Total number of mention entries across all users.
    pub fn total_entries(&self) -> usize {
        self.index.values().map(|v| v.len()).sum()
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_mention() {
        let mentions = parse_mentions("Hello @alice!");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "alice");
        assert_eq!(mentions[0].offset, 6);
        assert_eq!(mentions[0].length, 6); // @alice
    }

    #[test]
    fn parse_multiple_mentions() {
        let mentions = parse_mentions("@alice and @bob should review");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].username, "alice");
        assert_eq!(mentions[1].username, "bob");
    }

    #[test]
    fn parse_hyphenated_username() {
        let mentions = parse_mentions("Hey @bob-smith, check this");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "bob-smith");
    }

    #[test]
    fn parse_dotted_username() {
        let mentions = parse_mentions("@jane.doe reviewed");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "jane.doe");
    }

    #[test]
    fn parse_underscore_username() {
        let mentions = parse_mentions("@_internal_user go");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "_internal_user");
    }

    #[test]
    fn parse_no_mentions() {
        let mentions = parse_mentions("No mentions here at all");
        assert!(mentions.is_empty());
    }

    #[test]
    fn parse_email_not_mention() {
        // @ preceded by alnum is part of email, but we parse from @
        let mentions = parse_mentions("Contact user@example.com");
        // Our parser will pick up @example — fair trade-off for simplicity
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "example.com");
    }

    #[test]
    fn parse_at_end_of_string() {
        let mentions = parse_mentions("cc @dave");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].username, "dave");
    }

    #[test]
    fn parse_consecutive_mentions() {
        let mentions = parse_mentions("@alice@bob");
        // Parser sees @alice then @bob as separate tokens
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].username, "alice");
        assert_eq!(mentions[1].username, "bob");
    }

    #[test]
    fn parse_at_alone_ignored() {
        let mentions = parse_mentions("@ not a mention");
        assert!(mentions.is_empty());
    }

    #[test]
    fn parse_numeric_start_ignored() {
        let mentions = parse_mentions("@123abc");
        assert!(mentions.is_empty());
    }

    #[test]
    fn resolve_mentions_from_directory() {
        let mut directory = HashMap::new();
        let alice_id = Uuid::from_bytes([1; 16]);
        let bob_id = Uuid::from_bytes([2; 16]);
        directory.insert("alice".to_string(), alice_id);
        directory.insert("bob".to_string(), bob_id);

        let mut mentions = parse_mentions("@alice and @bob and @unknown");
        resolve_mentions(&mut mentions, &directory);

        assert_eq!(mentions[0].user_id, Some(alice_id));
        assert_eq!(mentions[1].user_id, Some(bob_id));
        assert_eq!(mentions[2].user_id, None);
    }

    #[test]
    fn resolve_case_insensitive() {
        let mut directory = HashMap::new();
        let alice_id = Uuid::from_bytes([1; 16]);
        directory.insert("Alice".to_string(), alice_id);

        let mut mentions = parse_mentions("@alice"); // lowercase
        resolve_mentions(&mut mentions, &directory);
        assert_eq!(mentions[0].user_id, Some(alice_id));
    }

    #[test]
    fn mention_index_add_and_query() {
        let mut idx = MentionIndex::new();
        let user = Uuid::from_bytes([1; 16]);
        let tid = ThreadId::new();
        let cid = CommentId::new();

        idx.add_mention(user, tid, cid);
        assert_eq!(idx.mention_count(user), 1);
        assert_eq!(idx.mentions_of(user).len(), 1);
        assert_eq!(idx.mentions_of(user)[0], (tid, cid));
    }

    #[test]
    fn mention_index_remove() {
        let mut idx = MentionIndex::new();
        let user = Uuid::from_bytes([1; 16]);
        let tid = ThreadId::new();
        let cid1 = CommentId::new();
        let cid2 = CommentId::new();

        idx.add_mention(user, tid, cid1);
        idx.add_mention(user, tid, cid2);
        assert_eq!(idx.mention_count(user), 2);

        idx.remove_comment_mentions(tid, cid1);
        assert_eq!(idx.mention_count(user), 1);
    }

    #[test]
    fn mention_index_deduplication() {
        let mut idx = MentionIndex::new();
        let user = Uuid::from_bytes([1; 16]);
        let tid = ThreadId::new();
        let cid = CommentId::new();

        idx.add_mention(user, tid, cid);
        idx.add_mention(user, tid, cid); // duplicate
        assert_eq!(idx.mention_count(user), 1);
    }
}
