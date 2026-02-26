//! Data extraction — pull numeric values from the spreadsheet for charting.
//!
//! The [`DataResolver`] reads cell values through a [`CellDataProvider`]
//! (typically a `RecalcEngine` or `Spreadsheet`) and produces a
//! [`ResolvedChart`] with concrete `f64` vectors ready for layout.
//!
//! Errors are handled gracefully: non-numeric cells yield `None` so the
//! chart can still render partial data with gaps.

use crate::evaluator::CellDataProvider;
use crate::types::Value;
use super::types::{CategorySource, ChartKind, ChartSpec, DataSeries, StackMode};

// ---------------------------------------------------------------------------
// Resolved data
// ---------------------------------------------------------------------------

/// One resolved data series — numeric values extracted from the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSeries {
    pub label: String,
    /// `None` entries represent missing/non-numeric cells.
    pub values: Vec<Option<f64>>,
}

impl ResolvedSeries {
    /// Sum of all non-None values.
    pub fn sum(&self) -> f64 {
        self.values.iter().filter_map(|v| *v).sum()
    }

    /// Count of non-None values.
    pub fn count(&self) -> usize {
        self.values.iter().filter(|v| v.is_some()).count()
    }

    /// Min of non-None values.
    pub fn min(&self) -> Option<f64> {
        self.values
            .iter()
            .filter_map(|v| *v)
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.min(v))))
    }

    /// Max of non-None values.
    pub fn max(&self) -> Option<f64> {
        self.values
            .iter()
            .filter_map(|v| *v)
            .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v))))
    }
}

/// Fully resolved chart data — ready for layout.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedChart {
    pub kind: ChartKind,
    pub title: Option<String>,
    pub series: Vec<ResolvedSeries>,
    pub categories: Vec<String>,
    /// Global min across all series.
    pub data_min: f64,
    /// Global max across all series.
    pub data_max: f64,
    /// Stack mode (affects how min/max are computed for stacked charts).
    pub stack_mode: StackMode,
}

impl ResolvedChart {
    /// Number of data points per series (= category count).
    pub fn point_count(&self) -> usize {
        self.categories.len()
    }

    /// Whether any series contains at least one value.
    pub fn has_data(&self) -> bool {
        self.series.iter().any(|s| s.count() > 0)
    }

    /// Compute stacked totals per category index (for stacked charts).
    pub fn stacked_totals(&self) -> Vec<f64> {
        let n = self.point_count();
        let mut totals = vec![0.0; n];
        for s in &self.series {
            for (i, v) in s.values.iter().enumerate() {
                if i < n {
                    totals[i] += v.unwrap_or(0.0);
                }
            }
        }
        totals
    }
}

// ---------------------------------------------------------------------------
// Data resolver
// ---------------------------------------------------------------------------

/// Extracts chart data from a cell data provider.
pub struct DataResolver;

impl DataResolver {
    /// Resolve a [`ChartSpec`] into a [`ResolvedChart`] by reading cell values.
    pub fn resolve(spec: &ChartSpec, provider: &dyn CellDataProvider) -> ResolvedChart {
        // 1. Resolve each series
        let resolved_series: Vec<ResolvedSeries> = spec
            .series
            .iter()
            .map(|ds| Self::resolve_series(ds, provider))
            .collect();

        // 2. Determine max point count
        let max_points = resolved_series.iter().map(|s| s.values.len()).max().unwrap_or(0);

        // 3. Resolve categories
        let categories = Self::resolve_categories(&spec.categories, max_points, provider);

        // 4. Compute global min/max
        let (data_min, data_max) =
            Self::compute_bounds(&resolved_series, &spec.y_axis, spec.stack_mode, max_points);

        ResolvedChart {
            kind: spec.kind,
            title: spec.title.clone(),
            series: resolved_series,
            categories,
            data_min,
            data_max,
            stack_mode: spec.stack_mode,
        }
    }

    /// Resolve a single data series.
    fn resolve_series(ds: &DataSeries, provider: &dyn CellDataProvider) -> ResolvedSeries {
        let coords = ds.cell_coords();
        let values: Vec<Option<f64>> = coords
            .iter()
            .map(|&(col, row)| {
                let val = provider.get_cell_value(col, row);
                Self::value_to_f64(&val)
            })
            .collect();
        ResolvedSeries {
            label: ds.label.clone(),
            values,
        }
    }

    /// Try to coerce a spreadsheet value to f64.
    fn value_to_f64(val: &Value) -> Option<f64> {
        match val {
            Value::Number(n) => {
                if n.is_finite() {
                    Some(*n)
                } else {
                    None
                }
            }
            Value::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
            Value::Text(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// Resolve category labels.
    fn resolve_categories(
        source: &CategorySource,
        count: usize,
        provider: &dyn CellDataProvider,
    ) -> Vec<String> {
        match source {
            CategorySource::Auto => (1..=count).map(|i| i.to_string()).collect(),
            CategorySource::Explicit(labels) => {
                let mut cats = labels.clone();
                // Pad or trim to match point count
                cats.resize(count, String::new());
                cats
            }
            CategorySource::Range(sc, sr, ec, er) => {
                let mut labels = Vec::new();
                for row in *sr..=*er {
                    for col in *sc..=*ec {
                        let val = provider.get_cell_value(col, row);
                        labels.push(Self::value_to_label(&val));
                    }
                }
                labels.resize(count, String::new());
                labels
            }
        }
    }

    /// Convert a cell value to a display label.
    fn value_to_label(val: &Value) -> String {
        match val {
            Value::Number(n) => {
                if *n == n.floor() && n.abs() < 1e12 {
                    format!("{}", *n as i64)
                } else {
                    format!("{:.2}", n)
                }
            }
            Value::Text(s) => s.clone(),
            Value::Boolean(b) => (if *b { "TRUE" } else { "FALSE" }).to_string(),
            Value::Empty => String::new(),
            _ => String::new(),
        }
    }

    /// Compute data bounds (min, max) considering stack mode and explicit axis config.
    fn compute_bounds(
        series: &[ResolvedSeries],
        y_axis: &super::types::AxisConfig,
        stack_mode: StackMode,
        point_count: usize,
    ) -> (f64, f64) {
        // If explicit axis range, use that
        if let (Some(min), Some(max)) = (y_axis.min, y_axis.max) {
            return (min, max);
        }

        let (raw_min, raw_max) = match stack_mode {
            StackMode::None => {
                let mut gmin = f64::INFINITY;
                let mut gmax = f64::NEG_INFINITY;
                for s in series {
                    if let Some(v) = s.min() {
                        gmin = gmin.min(v);
                    }
                    if let Some(v) = s.max() {
                        gmax = gmax.max(v);
                    }
                }
                if gmin.is_infinite() {
                    (0.0, 1.0) // no data
                } else {
                    (gmin, gmax)
                }
            }
            StackMode::Stacked => {
                let mut max_stack = 0.0_f64;
                let mut min_val = 0.0_f64;
                for i in 0..point_count {
                    let stack: f64 = series
                        .iter()
                        .map(|s| s.values.get(i).copied().flatten().unwrap_or(0.0))
                        .sum();
                    max_stack = max_stack.max(stack);
                }
                // Allow negative stacks
                for s in series {
                    if let Some(v) = s.min() {
                        min_val = min_val.min(v);
                    }
                }
                (min_val.min(0.0), max_stack)
            }
            StackMode::PercentStacked => (0.0, 100.0),
        };

        // Apply explicit overrides (partial)
        let final_min = y_axis.min.unwrap_or(raw_min.min(0.0));
        let final_max = y_axis.max.unwrap_or(raw_max);

        if (final_max - final_min).abs() < f64::EPSILON {
            (final_min, final_min + 1.0)
        } else {
            (final_min, final_max)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Spreadsheet;

    fn sheet_with_column(vals: &[f64]) -> Spreadsheet {
        let mut s = Spreadsheet::new(10, 10);
        for (i, v) in vals.iter().enumerate() {
            s.set_cell(0, i as u32, Value::Number(*v));
        }
        s
    }

    #[test]
    fn resolve_simple_bar() {
        let sheet = sheet_with_column(&[10.0, 20.0, 30.0]);
        let spec = ChartSpec::bar(1, "Sales", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, &sheet);

        assert_eq!(resolved.series.len(), 1);
        assert_eq!(resolved.series[0].values, vec![Some(10.0), Some(20.0), Some(30.0)]);
        assert_eq!(resolved.categories, vec!["1", "2", "3"]);
    }

    #[test]
    fn resolve_with_empty_cells() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(5.0));
        // row 1 is empty
        sheet.set_cell(0, 2, Value::Number(15.0));

        let spec = ChartSpec::bar(1, "Gaps", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, &sheet);

        assert_eq!(
            resolved.series[0].values,
            vec![Some(5.0), None, Some(15.0)]
        );
    }

    #[test]
    fn resolve_with_text_cells() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(10.0));
        sheet.set_cell(0, 1, Value::Text("hello".into())); // non-numeric
        sheet.set_cell(0, 2, Value::Text("42".into()));     // parseable

        let spec = ChartSpec::bar(1, "Mixed", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, &sheet);

        assert_eq!(
            resolved.series[0].values,
            vec![Some(10.0), None, Some(42.0)]
        );
    }

    #[test]
    fn resolve_boolean_as_number() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Boolean(true));
        sheet.set_cell(0, 1, Value::Boolean(false));

        let spec = ChartSpec::bar(1, "Bools", (0, 0, 0, 1));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.series[0].values, vec![Some(1.0), Some(0.0)]);
    }

    #[test]
    fn data_min_max() {
        let sheet = sheet_with_column(&[10.0, 50.0, 30.0, 5.0]);
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 3));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.data_min, 0.0); // min(0, 5) → origin starts at 0
        assert_eq!(resolved.data_max, 50.0);
    }

    #[test]
    fn data_min_max_negative() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(-20.0));
        sheet.set_cell(0, 1, Value::Number(10.0));

        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 1));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.data_min, -20.0);
        assert_eq!(resolved.data_max, 10.0);
    }

    #[test]
    fn explicit_categories() {
        let sheet = sheet_with_column(&[10.0, 20.0]);
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 1))
            .with_categories(CategorySource::Explicit(vec!["Q1".into(), "Q2".into()]));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.categories, vec!["Q1", "Q2"]);
    }

    #[test]
    fn range_categories() {
        let mut sheet = Spreadsheet::new(10, 10);
        // Data in A column
        sheet.set_cell(0, 0, Value::Number(10.0));
        sheet.set_cell(0, 1, Value::Number(20.0));
        // Labels in B column
        sheet.set_cell(1, 0, Value::Text("Jan".into()));
        sheet.set_cell(1, 1, Value::Text("Feb".into()));

        let spec = ChartSpec::bar(1, "Revenue", (0, 0, 0, 1))
            .with_categories(CategorySource::Range(1, 0, 1, 1));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.categories, vec!["Jan", "Feb"]);
    }

    #[test]
    fn multi_series() {
        let mut sheet = Spreadsheet::new(10, 10);
        // Series 1 in column A
        sheet.set_cell(0, 0, Value::Number(10.0));
        sheet.set_cell(0, 1, Value::Number(20.0));
        // Series 2 in column B
        sheet.set_cell(1, 0, Value::Number(30.0));
        sheet.set_cell(1, 1, Value::Number(40.0));

        let spec = ChartSpec::bar(1, "S1", (0, 0, 0, 1))
            .with_series(DataSeries::new("S2", (1, 0, 1, 1)));
        let resolved = DataResolver::resolve(&spec, &sheet);

        assert_eq!(resolved.series.len(), 2);
        assert_eq!(resolved.series[0].values, vec![Some(10.0), Some(20.0)]);
        assert_eq!(resolved.series[1].values, vec![Some(30.0), Some(40.0)]);
    }

    #[test]
    fn stacked_bounds() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(10.0));
        sheet.set_cell(0, 1, Value::Number(20.0));
        sheet.set_cell(1, 0, Value::Number(5.0));
        sheet.set_cell(1, 1, Value::Number(15.0));

        let spec = ChartSpec::bar(1, "S1", (0, 0, 0, 1))
            .with_series(DataSeries::new("S2", (1, 0, 1, 1)))
            .with_stack(StackMode::Stacked);
        let resolved = DataResolver::resolve(&spec, &sheet);
        // Stack totals: cat0 = 10+5 = 15, cat1 = 20+15 = 35
        assert_eq!(resolved.data_max, 35.0);
    }

    #[test]
    fn percent_stacked_bounds() {
        let sheet = sheet_with_column(&[10.0, 20.0]);
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 1))
            .with_stack(StackMode::PercentStacked);
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.data_min, 0.0);
        assert_eq!(resolved.data_max, 100.0);
    }

    #[test]
    fn explicit_axis_range_overrides() {
        let sheet = sheet_with_column(&[10.0, 20.0, 30.0]);
        let spec = ChartSpec::bar(1, "X", (0, 0, 0, 2))
            .with_y_axis(
                super::super::types::AxisConfig::default().with_range(0.0, 100.0),
            );
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.data_min, 0.0);
        assert_eq!(resolved.data_max, 100.0);
    }

    #[test]
    fn resolved_series_stats() {
        let s = ResolvedSeries {
            label: "Test".into(),
            values: vec![Some(10.0), None, Some(30.0), Some(20.0)],
        };
        assert_eq!(s.sum(), 60.0);
        assert_eq!(s.count(), 3);
        assert_eq!(s.min(), Some(10.0));
        assert_eq!(s.max(), Some(30.0));
    }

    #[test]
    fn empty_chart_has_data() {
        let sheet = Spreadsheet::new(10, 10);
        let spec = ChartSpec::bar(1, "Empty", (0, 0, 0, 2));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert!(!resolved.has_data());
    }

    #[test]
    fn stacked_totals() {
        let mut sheet = Spreadsheet::new(10, 10);
        sheet.set_cell(0, 0, Value::Number(10.0));
        sheet.set_cell(0, 1, Value::Number(20.0));
        sheet.set_cell(1, 0, Value::Number(5.0));
        sheet.set_cell(1, 1, Value::Number(15.0));

        let spec = ChartSpec::bar(1, "S1", (0, 0, 0, 1))
            .with_series(DataSeries::new("S2", (1, 0, 1, 1)));
        let resolved = DataResolver::resolve(&spec, &sheet);
        assert_eq!(resolved.stacked_totals(), vec![15.0, 35.0]);
    }
}
