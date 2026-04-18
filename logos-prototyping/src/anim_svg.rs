//! # Animated SVG Reader
//!
//! Parses `<animate>` and `<animateTransform>` elements from an SVG string
//! and converts them into [`Timeline`] + [`Keyframe`] sequences.
//!
//! ## Supported SVG animation elements
//! - `<animate>` on `opacity`, `x`, `y`, `width`, `height`, `fill-opacity`, `cx`, `cy`
//! - `<animateTransform>` with `type="translate"`, `type="rotate"`, `type="scale"`
//!
//! The parser is intentionally lightweight — it uses a regex-free, line-oriented
//! tokeniser to avoid pulling in heavy XML crates.

use uuid::Uuid;

use crate::animate::{AnimationValue, EasingCurve};
use crate::timeline::{Keyframe, LoopMode, Timeline};

// ── Errors ────────────────────────────────────────────────────────────────────

/// Error type for the animated SVG reader.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnimSvgError {
    #[error("No <animate> or <animateTransform> elements found in SVG")]
    NoAnimations,
    #[error("Malformed timing: '{0}'")]
    BadTiming(String),
    #[error("Unsupported animateTransform type: '{0}'")]
    UnsupportedTransformType(String),
}

// ── Parsed SVG animation record ───────────────────────────────────────────────

/// One `<animate>` or `<animateTransform>` element, parsed into a flat record.
#[derive(Debug, Clone)]
pub struct SvgAnimRecord {
    /// Target element id (`attributeName` value or `xlink:href` without `#`).
    pub target_id: String,
    /// Property being animated (e.g. `"opacity"`, `"transform.translate"`).
    pub property: String,
    /// Begin time in milliseconds.
    pub begin_ms: u64,
    /// Duration in milliseconds.
    pub dur_ms: u64,
    /// Starting value.
    pub from_value: AnimationValue,
    /// Ending value.
    pub to_value: AnimationValue,
    /// Whether the animation fills forward.
    pub fill_freeze: bool,
    /// `repeatCount` ("indefinite" → u32::MAX).
    pub repeat_count: u32,
}

// ── Reader ────────────────────────────────────────────────────────────────────

/// Reads `<animate>` and `<animateTransform>` from an SVG source string and
/// returns one [`Timeline`] per unique target element id.
pub struct AnimSvgReader;

impl AnimSvgReader {
    /// Parse `svg_source` and produce timelines.
    pub fn read(svg_source: &str) -> Result<Vec<AnimSvgResult>, AnimSvgError> {
        let records = Self::parse_records(svg_source)?;
        if records.is_empty() {
            return Err(AnimSvgError::NoAnimations);
        }

        // Group records by target_id.
        let mut groups: std::collections::BTreeMap<String, Vec<SvgAnimRecord>> =
            std::collections::BTreeMap::new();
        for rec in records {
            groups.entry(rec.target_id.clone()).or_default().push(rec);
        }

        let mut results = Vec::new();
        for (target_id, recs) in groups {
            // duration = max(begin + dur) across all records
            let duration_ms = recs
                .iter()
                .map(|r| r.begin_ms + r.dur_ms)
                .max()
                .unwrap_or(1000);

            let uuid = Uuid::new_v4();
            let mut timeline = Timeline::new(&target_id, uuid, duration_ms);

            for rec in &recs {
                let kf_start = Keyframe::new(rec.begin_ms, &rec.property, rec.from_value.clone());
                let kf_end = Keyframe::new(
                    rec.begin_ms + rec.dur_ms,
                    &rec.property,
                    rec.to_value.clone(),
                );
                timeline.add_keyframe(kf_start);
                timeline.add_keyframe(kf_end);

                if rec.fill_freeze {
                    timeline.loop_mode = LoopMode::Once;
                }
                if rec.repeat_count == u32::MAX {
                    timeline.loop_mode = LoopMode::Loop;
                }
            }

            results.push(AnimSvgResult {
                target_element_id: target_id,
                timeline,
            });
        }

        Ok(results)
    }

    // ── Internal parser ───────────────────────────────────────────────────────

    fn parse_records(svg: &str) -> Result<Vec<SvgAnimRecord>, AnimSvgError> {
        let mut records = Vec::new();

        // Split on `<` to get a stream of tag-like fragments.
        for fragment in svg.split('<') {
            let trimmed = fragment.trim();

            let (is_animate, is_transform) = if trimmed.starts_with("animate ") || trimmed.starts_with("animate>") {
                (true, false)
            } else if trimmed.starts_with("animateTransform ") {
                (false, true)
            } else {
                continue;
            };

            let attrs = parse_attrs(trimmed);

            let target = attrs.get("id")
                .or_else(|| attrs.get("xlink:href"))
                .map(|s| s.trim_start_matches('#').to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let prop = if is_transform {
                let ttype = attrs.get("type").map(|s| s.as_str()).unwrap_or("translate");
                match ttype {
                    "translate" => "transform.translate".to_string(),
                    "rotate"    => "transform.rotation".to_string(),
                    "scale"     => "transform.scale".to_string(),
                    other => return Err(AnimSvgError::UnsupportedTransformType(other.to_string())),
                }
            } else {
                attrs.get("attributeName").cloned().unwrap_or_else(|| "unknown".to_string())
            };

            let begin_ms = parse_time_ms(attrs.get("begin").map(|s| s.as_str()).unwrap_or("0s"))
                .map_err(|e| AnimSvgError::BadTiming(e))?;
            let dur_ms = parse_time_ms(attrs.get("dur").map(|s| s.as_str()).unwrap_or("1s"))
                .map_err(|e| AnimSvgError::BadTiming(e))?;

            let from_str = attrs.get("from").map(|s| s.as_str()).unwrap_or("0");
            let to_str   = attrs.get("to").map(|s| s.as_str()).unwrap_or("0");

            let from_value = parse_anim_value(from_str, &prop);
            let to_value   = parse_anim_value(to_str, &prop);

            let fill_freeze = attrs.get("fill").map(|s| s == "freeze").unwrap_or(false);
            let repeat_count = match attrs.get("repeatCount").map(|s| s.as_str()) {
                Some("indefinite") => u32::MAX,
                Some(n) => n.parse::<u32>().unwrap_or(1),
                None => 1,
            };

            records.push(SvgAnimRecord {
                target_id: target,
                property: prop,
                begin_ms,
                dur_ms,
                from_value,
                to_value,
                fill_freeze,
                repeat_count,
            });
        }

        Ok(records)
    }
}

// ── Result type ──────────────────────────────────────────────────────────────

/// One timeline produced from an SVG target element.
#[derive(Debug)]
pub struct AnimSvgResult {
    pub target_element_id: String,
    pub timeline: Timeline,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Tokenise `key="value"` pairs from a tag fragment.
fn parse_attrs(fragment: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut rest = fragment;
    while let Some(eq_pos) = rest.find('=') {
        let key = rest[..eq_pos].split_whitespace().last().unwrap_or("").to_string();
        rest = &rest[eq_pos + 1..];
        let (val, after) = if rest.starts_with('"') {
            let end = rest[1..].find('"').map(|i| i + 1).unwrap_or(rest.len() - 1);
            (&rest[1..end], &rest[end + 1..])
        } else if rest.starts_with('\'') {
            let end = rest[1..].find('\'').map(|i| i + 1).unwrap_or(rest.len() - 1);
            (&rest[1..end], &rest[end + 1..])
        } else {
            let end = rest.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(rest.len());
            (&rest[..end], &rest[end..])
        };
        if !key.is_empty() {
            map.insert(key, val.to_string());
        }
        rest = after;
    }
    map
}

/// Parse an SVG time string (e.g. `"2s"`, `"500ms"`, `"1.5"`) to milliseconds.
fn parse_time_ms(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if s.ends_with("ms") {
        s[..s.len() - 2].trim().parse::<f64>()
            .map(|v| v.max(0.0) as u64)
            .map_err(|_| s.to_string())
    } else if s.ends_with('s') {
        s[..s.len() - 1].trim().parse::<f64>()
            .map(|v| (v * 1000.0).max(0.0) as u64)
            .map_err(|_| s.to_string())
    } else {
        // bare number treated as seconds
        s.parse::<f64>()
            .map(|v| (v * 1000.0).max(0.0) as u64)
            .map_err(|_| s.to_string())
    }
}

/// Convert a string value to an `AnimationValue` based on property name.
fn parse_anim_value(s: &str, prop: &str) -> AnimationValue {
    let s = s.trim();
    // transform.translate / transform.scale → Point
    if prop.contains("translate") || prop.contains("scale") {
        let nums: Vec<f64> = s.split(|c: char| c == ',' || c.is_whitespace())
            .filter_map(|t| t.trim().parse::<f64>().ok())
            .collect();
        match nums.len() {
            0 => AnimationValue::Point(0.0, 0.0),
            1 => AnimationValue::Point(nums[0], nums[0]),
            _ => AnimationValue::Point(nums[0], nums[1]),
        }
    } else {
        // scalar (opacity, rotation, x, y, etc.)
        let v = s.parse::<f64>().unwrap_or(0.0);
        // percent → fraction
        if s.ends_with('%') {
            AnimationValue::Scalar(s[..s.len()-1].trim().parse::<f64>().unwrap_or(0.0) / 100.0)
        } else {
            AnimationValue::Scalar(v)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anim_svg_no_animations_error() {
        let err = AnimSvgReader::read("<svg><rect/></svg>").unwrap_err();
        assert_eq!(err, AnimSvgError::NoAnimations);
    }

    #[test]
    fn parse_time_seconds() {
        assert_eq!(parse_time_ms("2s").unwrap(), 2000);
    }

    #[test]
    fn parse_time_milliseconds() {
        assert_eq!(parse_time_ms("500ms").unwrap(), 500);
    }
}
