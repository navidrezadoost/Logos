//! Chart styling — colours, fonts, padding, and theme integration.
//!
//! [`ChartStyle`] is the resolved visual configuration for a chart. It
//! controls spacing, font sizes, series colouring, and background.  A
//! [`ChartTheme`] enum provides pre-built palettes and can be extended
//! with custom overrides.

use super::types::{AxisConfig, SeriesColor, DEFAULT_PALETTE};

// ---------------------------------------------------------------------------
// Theme presets
// ---------------------------------------------------------------------------

/// Pre-built themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartTheme {
    /// Default Material-inspired palette.
    Default,
    /// Monochrome (shades of a single hue).
    Monochrome,
    /// Pastel colours.
    Pastel,
    /// High-contrast for accessibility.
    HighContrast,
}

impl ChartTheme {
    /// Generate a palette of `n` colours for this theme.
    pub fn palette(self, n: usize) -> Vec<SeriesColor> {
        match self {
            ChartTheme::Default => (0..n).map(|i| DEFAULT_PALETTE[i % DEFAULT_PALETTE.len()]).collect(),
            ChartTheme::Monochrome => {
                // Shades of blue from light to dark
                (0..n)
                    .map(|i| {
                        let t = if n <= 1 { 0.5 } else { i as f64 / (n - 1) as f64 };
                        let v = (255.0 * (1.0 - t * 0.7)) as u8;
                        SeriesColor::rgb(
                            (66.0 + (v as f64 - 66.0) * 0.3) as u8,
                            (133.0 + (v as f64 - 133.0) * 0.3) as u8,
                            v,
                        )
                    })
                    .collect()
            }
            ChartTheme::Pastel => {
                let pastel = [
                    SeriesColor::rgb(174, 198, 255),
                    SeriesColor::rgb(255, 179, 186),
                    SeriesColor::rgb(255, 223, 186),
                    SeriesColor::rgb(186, 255, 201),
                    SeriesColor::rgb(255, 255, 186),
                    SeriesColor::rgb(216, 186, 255),
                    SeriesColor::rgb(186, 255, 255),
                    SeriesColor::rgb(220, 220, 220),
                ];
                (0..n).map(|i| pastel[i % pastel.len()]).collect()
            }
            ChartTheme::HighContrast => {
                let hc = [
                    SeriesColor::rgb(0, 0, 0),
                    SeriesColor::rgb(255, 0, 0),
                    SeriesColor::rgb(0, 0, 255),
                    SeriesColor::rgb(0, 180, 0),
                    SeriesColor::rgb(255, 165, 0),
                    SeriesColor::rgb(128, 0, 128),
                    SeriesColor::rgb(0, 128, 128),
                    SeriesColor::rgb(128, 128, 0),
                ];
                (0..n).map(|i| hc[i % hc.len()]).collect()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Spacing / layout constants
// ---------------------------------------------------------------------------

/// Padding and sizing constants for chart layout.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartPadding {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

impl Default for ChartPadding {
    fn default() -> Self {
        Self {
            top: 40.0,    // room for title
            right: 20.0,
            bottom: 50.0, // room for x-axis labels
            left: 60.0,   // room for y-axis labels
        }
    }
}

impl ChartPadding {
    /// Symmetric padding.
    pub fn uniform(v: f64) -> Self {
        Self {
            top: v,
            right: v,
            bottom: v,
            left: v,
        }
    }
}

// ---------------------------------------------------------------------------
// Font config
// ---------------------------------------------------------------------------

/// Font sizing for chart text elements.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartFonts {
    pub title_size: f32,
    pub axis_title_size: f32,
    pub axis_label_size: f32,
    pub legend_size: f32,
    pub data_label_size: f32,
}

impl Default for ChartFonts {
    fn default() -> Self {
        Self {
            title_size: 16.0,
            axis_title_size: 12.0,
            axis_label_size: 10.0,
            legend_size: 10.0,
            data_label_size: 9.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Full chart style
// ---------------------------------------------------------------------------

/// Complete visual configuration for a chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartStyle {
    pub theme: ChartTheme,
    pub padding: ChartPadding,
    pub fonts: ChartFonts,
    pub background: SeriesColor,
    pub plot_background: SeriesColor,
    pub grid_color: SeriesColor,
    pub text_color: SeriesColor,
    pub border_color: SeriesColor,
    /// Whether to show data labels on bars/slices.
    pub show_data_labels: bool,
    /// Bar gap as a fraction of bar width (0.0 = no gap, 1.0 = gap == bar width).
    pub bar_gap: f64,
    /// Gap between groups of bars (for multi-series bar charts).
    pub bar_group_gap: f64,
    /// Line thickness for line/area charts.
    pub line_width: f32,
    /// Marker radius for line/scatter charts (0 = no markers).
    pub marker_radius: f32,
    /// Pie chart: start angle in degrees (0 = 3 o'clock / east).
    pub pie_start_angle: f64,
    /// Pie chart: hole radius as fraction of outer radius (0 = pie, >0 = donut).
    pub donut_hole: f64,
    /// Y-axis display configuration.
    pub y_axis: AxisConfig,
    /// X-axis display configuration.
    pub x_axis: AxisConfig,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            theme: ChartTheme::Default,
            padding: ChartPadding::default(),
            fonts: ChartFonts::default(),
            background: SeriesColor::rgb(255, 255, 255),
            plot_background: SeriesColor::rgb(250, 250, 250),
            grid_color: SeriesColor::new(200, 200, 200, 128),
            text_color: SeriesColor::rgb(51, 51, 51),
            border_color: SeriesColor::new(200, 200, 200, 255),
            show_data_labels: false,
            bar_gap: 0.2,
            bar_group_gap: 0.3,
            line_width: 2.0,
            marker_radius: 4.0,
            pie_start_angle: -90.0,
            donut_hole: 0.0,
            y_axis: AxisConfig::default(),
            x_axis: AxisConfig::default(),
        }
    }
}

impl ChartStyle {
    /// Get resolved series colours (considering per-series overrides + theme).
    pub fn series_colors(
        &self,
        series_count: usize,
        overrides: &[Option<SeriesColor>],
    ) -> Vec<SeriesColor> {
        let palette = self.theme.palette(series_count);
        (0..series_count)
            .map(|i| {
                overrides
                    .get(i)
                    .copied()
                    .flatten()
                    .unwrap_or_else(|| palette[i])
            })
            .collect()
    }

    /// Builder: set theme.
    pub fn with_theme(mut self, theme: ChartTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Builder: show data labels.
    pub fn with_data_labels(mut self, show: bool) -> Self {
        self.show_data_labels = show;
        self
    }

    /// Builder: donut hole ratio.
    pub fn with_donut_hole(mut self, ratio: f64) -> Self {
        self.donut_hole = ratio.clamp(0.0, 0.9);
        self
    }

    /// Builder: bar gap ratio.
    pub fn with_bar_gap(mut self, gap: f64) -> Self {
        self.bar_gap = gap.clamp(0.0, 2.0);
        self
    }

    /// Builder: line width.
    pub fn with_line_width(mut self, w: f32) -> Self {
        self.line_width = w.max(0.5);
        self
    }

    /// Builder: marker radius.
    pub fn with_marker_radius(mut self, r: f32) -> Self {
        self.marker_radius = r.max(0.0);
        self
    }

    /// The usable plot area within a chart of given total size.
    pub fn plot_rect(&self, width: f64, height: f64) -> (f64, f64, f64, f64) {
        let x = self.padding.left;
        let y = self.padding.top;
        let w = (width - self.padding.left - self.padding.right).max(1.0);
        let h = (height - self.padding.top - self.padding.bottom).max(1.0);
        (x, y, w, h)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_palette() {
        let p = ChartTheme::Default.palette(3);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0], DEFAULT_PALETTE[0]);
    }

    #[test]
    fn monochrome_palette_gradient() {
        let p = ChartTheme::Monochrome.palette(4);
        assert_eq!(p.len(), 4);
        // Later colours should be darker (lower blue channel)
        assert!(p[0].b > p[3].b);
    }

    #[test]
    fn pastel_palette() {
        let p = ChartTheme::Pastel.palette(2);
        assert_eq!(p.len(), 2);
        // Pastel colours have high channel values
        assert!(p[0].r > 100);
    }

    #[test]
    fn high_contrast_palette() {
        let p = ChartTheme::HighContrast.palette(2);
        assert_eq!(p[0], SeriesColor::rgb(0, 0, 0)); // black first
    }

    #[test]
    fn series_colors_with_override() {
        let style = ChartStyle::default();
        let red = SeriesColor::rgb(255, 0, 0);
        let colors = style.series_colors(3, &[None, Some(red), None]);
        assert_eq!(colors[1], red);
        assert_eq!(colors[0], DEFAULT_PALETTE[0]); // from theme
    }

    #[test]
    fn plot_rect() {
        let style = ChartStyle::default();
        let (x, y, w, h) = style.plot_rect(400.0, 300.0);
        assert_eq!(x, 60.0);  // left padding
        assert_eq!(y, 40.0);  // top padding
        assert_eq!(w, 320.0); // 400 - 60 - 20
        assert_eq!(h, 210.0); // 300 - 40 - 50
    }

    #[test]
    fn donut_hole_clamped() {
        let style = ChartStyle::default().with_donut_hole(1.5);
        assert_eq!(style.donut_hole, 0.9);
    }

    #[test]
    fn bar_gap_clamped() {
        let style = ChartStyle::default().with_bar_gap(-0.5);
        assert_eq!(style.bar_gap, 0.0);
    }

    #[test]
    fn line_width_min() {
        let style = ChartStyle::default().with_line_width(0.1);
        assert_eq!(style.line_width, 0.5);
    }

    #[test]
    fn uniform_padding() {
        let p = ChartPadding::uniform(10.0);
        assert_eq!(p.top, 10.0);
        assert_eq!(p.right, 10.0);
        assert_eq!(p.bottom, 10.0);
        assert_eq!(p.left, 10.0);
    }

    #[test]
    fn default_fonts() {
        let f = ChartFonts::default();
        assert!(f.title_size > f.axis_label_size);
    }
}
