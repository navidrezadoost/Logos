// SPDX-License-Identifier: MPL-2.0
// logos-layout/src/data_table.rs — Tabular data-source management
//
//  A `DataSourceTable` is a named container that groups a set of
//  [`DataSource`] columns (each targeting a different property path) so that
//  the entire table can be applied to a [`RepeatGrid`] in a single call.
//
//  Conceptually it mirrors a spreadsheet where:
//    • each *column* = one `DataSource` (name, property path, values)
//    • each *row*    = one set of values across all columns for that cell
//
//  This makes it possible to model rich per-cell datasets and export /
//  import them as plain table data.

use crate::repeat_grid::{DataSource, RepeatGrid};

// ═══════════════════════════════════════════════════════════════════════════
// DataSourceTable
// ═══════════════════════════════════════════════════════════════════════════

/// A named table of [`DataSource`] columns that can be applied to a
/// [`RepeatGrid`] all at once.
///
/// # Example
/// ```rust,ignore
/// let mut table = DataSourceTable::new("product_cards");
/// table.add_column(DataSource::new("title",  "label.text",  vec!["A", "B", "C"]));
/// table.add_column(DataSource::new("price",  "price.text",  vec!["$1", "$2", "$3"]));
/// table.apply_to_grid(&mut grid);          // attaches every column
/// ```
#[derive(Clone, Debug)]
pub struct DataSourceTable {
    /// Human-readable name for the table (used in UI and serialization).
    pub name: String,
    /// Ordered list of data-source columns.
    pub sources: Vec<DataSource>,
}

impl DataSourceTable {
    /// Create a new, empty table with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sources: Vec::new(),
        }
    }

    // ── Column operations ──────────────────────────────────────────────────

    /// Append a column to the table.
    ///
    /// If a column with the same `name` already exists it is *replaced*.
    pub fn add_column(&mut self, source: DataSource) {
        if let Some(pos) = self.sources.iter().position(|s| s.name == source.name) {
            self.sources[pos] = source;
        } else {
            self.sources.push(source);
        }
    }

    /// Remove the column whose `name` matches `col_name`.
    ///
    /// Returns the removed column if it existed, `None` otherwise.
    pub fn remove_column(&mut self, col_name: &str) -> Option<DataSource> {
        if let Some(pos) = self.sources.iter().position(|s| s.name == col_name) {
            Some(self.sources.remove(pos))
        } else {
            None
        }
    }

    /// Number of columns in the table.
    pub fn column_count(&self) -> usize {
        self.sources.len()
    }

    /// Get a column by name.
    pub fn column(&self, name: &str) -> Option<&DataSource> {
        self.sources.iter().find(|s| s.name == name)
    }

    /// Get a mutable reference to a column by name.
    pub fn column_mut(&mut self, name: &str) -> Option<&mut DataSource> {
        self.sources.iter_mut().find(|s| s.name == name)
    }

    // ── Row / cell access ──────────────────────────────────────────────────

    /// Number of data rows in the table.
    ///
    /// Defined as the maximum number of values across all columns.
    /// Returns `0` if there are no columns.
    pub fn row_count(&self) -> usize {
        self.sources.iter().map(|s| s.len()).max().unwrap_or(0)
    }

    /// Return a *header row* — the ordered list of column names.
    pub fn header_row(&self) -> Vec<&str> {
        self.sources.iter().map(|s| s.name.as_str()).collect()
    }

    /// Return one data row as an ordered list of `(column_name, value)` pairs.
    ///
    /// If `row` exceeds the values for a column, the column's values *cycle*
    /// (matching [`DataSource::value_for_index`] semantics).
    /// Returns an empty `Vec` when the table has no columns.
    pub fn data_row(&self, row: usize) -> Vec<(&str, &serde_json::Value)> {
        self.sources
            .iter()
            .map(|s| (s.name.as_str(), s.value_for_index(row)))
            .collect()
    }

    /// Return the value for a specific cell identified by `(row, col_name)`.
    ///
    /// Returns `None` if the column does not exist.  Cycles through column
    /// values when `row` exceeds column length.
    pub fn cell_value(&self, row: usize, col_name: &str) -> Option<&serde_json::Value> {
        self.column(col_name).map(|s| s.value_for_index(row))
    }

    // ── Grid integration ────────────────────────────────────────────────────

    /// Attach every column in this table to `grid` by calling
    /// [`RepeatGrid::attach_data_source`] for each source.
    ///
    /// Existing sources with matching names on the grid are replaced
    /// (delegate behaviour from `RepeatGrid::attach_data_source`).
    pub fn apply_to_grid(&self, grid: &mut RepeatGrid) {
        for source in &self.sources {
            grid.attach_data_source(source.clone());
        }
    }

    /// Detach every column in this table from `grid` by name.
    pub fn detach_from_grid(&self, grid: &mut RepeatGrid) {
        for source in &self.sources {
            grid.detach_data_source(&source.name);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text_col(name: &str, vals: &[&str]) -> DataSource {
        DataSource::new(name, "label.text", vals.iter().map(|v| json!(*v)).collect())
    }

    fn num_col(name: &str, vals: &[i64]) -> DataSource {
        DataSource::new(name, "price.text", vals.iter().map(|v| json!(*v)).collect())
    }

    #[test]
    fn test_new_table_is_empty() {
        let t = DataSourceTable::new("cards");
        assert_eq!(t.column_count(), 0);
        assert_eq!(t.row_count(), 0);
        assert!(t.header_row().is_empty());
    }

    #[test]
    fn test_add_column_increases_count() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["A", "B"]));
        assert_eq!(t.column_count(), 1);
        t.add_column(text_col("subtitle", &["X", "Y"]));
        assert_eq!(t.column_count(), 2);
    }

    #[test]
    fn test_add_column_replaces_existing_name() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["A", "B"]));
        t.add_column(text_col("title", &["X", "Y", "Z"]));
        assert_eq!(t.column_count(), 1);
        assert_eq!(t.column("title").unwrap().len(), 3);
    }

    #[test]
    fn test_remove_column_returns_source() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["A"]));
        let removed = t.remove_column("title");
        assert!(removed.is_some());
        assert_eq!(t.column_count(), 0);
    }

    #[test]
    fn test_remove_column_missing_returns_none() {
        let mut t = DataSourceTable::new("t");
        assert!(t.remove_column("nope").is_none());
    }

    #[test]
    fn test_row_count_is_max_of_column_lengths() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("a", &["X", "Y", "Z"]));       // 3
        t.add_column(num_col("b", &[1, 2]));                 // 2
        assert_eq!(t.row_count(), 3);
    }

    #[test]
    fn test_header_row_returns_column_names() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &[]));
        t.add_column(text_col("price", &[]));
        let headers = t.header_row();
        assert_eq!(headers, vec!["title", "price"]);
    }

    #[test]
    fn test_data_row_returns_values() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["Foo", "Bar"]));
        let row = t.data_row(0);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].0, "title");
        assert_eq!(row[0].1, &json!("Foo"));
    }

    #[test]
    fn test_data_row_cycles_for_short_column() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("x", &["A", "B"]));
        assert_eq!(t.data_row(2)[0].1, &json!("A")); // cycles
        assert_eq!(t.data_row(3)[0].1, &json!("B")); // cycles
    }

    #[test]
    fn test_cell_value_returns_correct_value() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["Hello", "World"]));
        assert_eq!(t.cell_value(0, "title"), Some(&json!("Hello")));
        assert_eq!(t.cell_value(1, "title"), Some(&json!("World")));
    }

    #[test]
    fn test_cell_value_missing_column_returns_none() {
        let t = DataSourceTable::new("t");
        assert!(t.cell_value(0, "ghost").is_none());
    }

    #[test]
    fn test_apply_to_grid_attaches_all_columns() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["A", "B", "C", "D"]));
        t.add_column(num_col("price", &[10, 20, 30, 40]));

        let mut grid = RepeatGrid::new(2, 2, 100.0, 80.0);
        assert_eq!(grid.source_count(), 0);
        t.apply_to_grid(&mut grid);
        assert_eq!(grid.source_count(), 2);
    }

    #[test]
    fn test_detach_from_grid_removes_columns() {
        let mut t = DataSourceTable::new("t");
        t.add_column(text_col("title", &["A", "B"]));
        t.add_column(num_col("price", &[1, 2]));

        let mut grid = RepeatGrid::new(2, 2, 100.0, 80.0);
        t.apply_to_grid(&mut grid);
        assert_eq!(grid.source_count(), 2);
        t.detach_from_grid(&mut grid);
        assert_eq!(grid.source_count(), 0);
    }
}
