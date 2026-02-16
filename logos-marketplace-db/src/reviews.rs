//! Reviews and ratings repository.

use crate::{DbError, DbResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A plugin review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: Uuid,
    pub plugin_id: Uuid,
    pub reviewer_id: Uuid,
    pub stars: u8,
    pub title: Option<String>,
    pub body: String,
    pub helpful_count: u32,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Review {
    pub fn new(plugin_id: Uuid, reviewer_id: Uuid, stars: u8, body: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or(std::time::Duration::ZERO)
            .as_secs();
        Self {
            id: Uuid::new_v4(),
            plugin_id,
            reviewer_id,
            stars: stars.clamp(1, 5),
            title: None,
            body: body.to_string(),
            helpful_count: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

/// Summary statistics for reviews on a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub plugin_id: Uuid,
    pub average_rating: f64,
    pub total_reviews: u32,
    pub rating_distribution: [u32; 5], // 1-star to 5-star counts
}

/// In-memory reviews repository.
pub struct ReviewRepo {
    reviews: HashMap<Uuid, Review>,
    /// plugin_id → review IDs
    by_plugin: HashMap<Uuid, Vec<Uuid>>,
    /// (plugin_id, reviewer_id) → review ID (prevent duplicates)
    by_pair: HashMap<(Uuid, Uuid), Uuid>,
}

impl ReviewRepo {
    pub fn new() -> Self {
        Self {
            reviews: HashMap::new(),
            by_plugin: HashMap::new(),
            by_pair: HashMap::new(),
        }
    }

    /// Insert a new review.
    pub fn insert(&mut self, review: Review) -> DbResult<Uuid> {
        let pair = (review.plugin_id, review.reviewer_id);
        if self.by_pair.contains_key(&pair) {
            return Err(DbError::Duplicate("one review per user per plugin".into()));
        }

        let id = review.id;
        self.by_pair.insert(pair, id);
        self.by_plugin.entry(review.plugin_id).or_default().push(id);
        self.reviews.insert(id, review);
        Ok(id)
    }

    /// Get a review by ID.
    pub fn get(&self, id: &Uuid) -> DbResult<&Review> {
        self.reviews.get(id).ok_or_else(|| DbError::NotFound(id.to_string()))
    }

    /// Get all reviews for a plugin.
    pub fn get_for_plugin(&self, plugin_id: &Uuid) -> Vec<&Review> {
        self.by_plugin
            .get(plugin_id)
            .map(|ids| ids.iter().filter_map(|id| self.reviews.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get review summary for a plugin.
    pub fn summary(&self, plugin_id: &Uuid) -> ReviewSummary {
        let reviews = self.get_for_plugin(plugin_id);
        let total = reviews.len() as u32;
        let mut distribution = [0u32; 5];
        let mut sum = 0.0f64;

        for r in &reviews {
            sum += r.stars as f64;
            if r.stars >= 1 && r.stars <= 5 {
                distribution[(r.stars - 1) as usize] += 1;
            }
        }

        ReviewSummary {
            plugin_id: *plugin_id,
            average_rating: if total > 0 { sum / total as f64 } else { 0.0 },
            total_reviews: total,
            rating_distribution: distribution,
        }
    }

    /// Mark a review as helpful.
    pub fn mark_helpful(&mut self, id: &Uuid) -> DbResult<()> {
        let review = self.reviews.get_mut(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        review.helpful_count += 1;
        Ok(())
    }

    /// Delete a review.
    pub fn delete(&mut self, id: &Uuid) -> DbResult<()> {
        let review = self.reviews.remove(id).ok_or_else(|| DbError::NotFound(id.to_string()))?;
        self.by_pair.remove(&(review.plugin_id, review.reviewer_id));
        if let Some(ids) = self.by_plugin.get_mut(&review.plugin_id) {
            ids.retain(|i| i != id);
        }
        Ok(())
    }

    /// Total review count.
    pub fn count(&self) -> usize {
        self.reviews.len()
    }
}

impl Default for ReviewRepo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_new() {
        let r = Review::new(Uuid::new_v4(), Uuid::new_v4(), 5, "Awesome!");
        assert_eq!(r.stars, 5);
        assert_eq!(r.body, "Awesome!");
    }

    #[test]
    fn test_review_clamp_stars() {
        let r = Review::new(Uuid::new_v4(), Uuid::new_v4(), 10, "Over");
        assert_eq!(r.stars, 5);

        let r2 = Review::new(Uuid::new_v4(), Uuid::new_v4(), 0, "Under");
        assert_eq!(r2.stars, 1);
    }

    #[test]
    fn test_review_repo_insert() {
        let mut repo = ReviewRepo::new();
        let r = Review::new(Uuid::new_v4(), Uuid::new_v4(), 4, "Good");
        assert!(repo.insert(r).is_ok());
        assert_eq!(repo.count(), 1);
    }

    #[test]
    fn test_review_repo_duplicate() {
        let mut repo = ReviewRepo::new();
        let plugin_id = Uuid::new_v4();
        let reviewer_id = Uuid::new_v4();

        repo.insert(Review::new(plugin_id, reviewer_id, 5, "First")).unwrap();
        let result = repo.insert(Review::new(plugin_id, reviewer_id, 3, "Second"));
        assert!(result.is_err());
    }

    #[test]
    fn test_review_repo_summary() {
        let mut repo = ReviewRepo::new();
        let plugin_id = Uuid::new_v4();

        repo.insert(Review::new(plugin_id, Uuid::new_v4(), 5, "Great")).unwrap();
        repo.insert(Review::new(plugin_id, Uuid::new_v4(), 3, "OK")).unwrap();
        repo.insert(Review::new(plugin_id, Uuid::new_v4(), 4, "Good")).unwrap();

        let summary = repo.summary(&plugin_id);
        assert_eq!(summary.total_reviews, 3);
        assert!((summary.average_rating - 4.0).abs() < 0.01);
        assert_eq!(summary.rating_distribution[4], 1); // 1x 5-star
        assert_eq!(summary.rating_distribution[3], 1); // 1x 4-star
        assert_eq!(summary.rating_distribution[2], 1); // 1x 3-star
    }

    #[test]
    fn test_review_repo_helpful() {
        let mut repo = ReviewRepo::new();
        let r = Review::new(Uuid::new_v4(), Uuid::new_v4(), 5, "Helpful review");
        let id = repo.insert(r).unwrap();

        repo.mark_helpful(&id).unwrap();
        repo.mark_helpful(&id).unwrap();
        assert_eq!(repo.get(&id).unwrap().helpful_count, 2);
    }

    #[test]
    fn test_review_repo_delete() {
        let mut repo = ReviewRepo::new();
        let r = Review::new(Uuid::new_v4(), Uuid::new_v4(), 1, "Bad");
        let id = repo.insert(r).unwrap();

        repo.delete(&id).unwrap();
        assert_eq!(repo.count(), 0);
    }
}
