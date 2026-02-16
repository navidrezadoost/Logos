//! Visual effect types (shadows, blurs).

use super::paint::Color;
use super::transform::Vector2D;
use serde::{Deserialize, Serialize};

/// The type of visual effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectType {
    /// An inner shadow.
    InnerShadow,
    /// A drop shadow.
    DropShadow,
    /// A layer (Gaussian) blur.
    LayerBlur,
    /// A background blur.
    BackgroundBlur,
}

/// A visual effect applied to a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    /// The type of effect.
    pub effect_type: EffectType,
    /// Whether the effect is visible.
    pub visible: bool,
    /// Blur radius (for all effect types).
    pub radius: f32,
    /// Shadow color (for shadow types).
    pub color: Option<Color>,
    /// Shadow offset (for shadow types).
    pub offset: Option<Vector2D>,
    /// Shadow spread (for shadow types).
    pub spread: f32,
}

impl Effect {
    /// Create a drop shadow effect.
    pub fn drop_shadow(color: Color, offset: Vector2D, radius: f32, spread: f32) -> Self {
        Self {
            effect_type: EffectType::DropShadow,
            visible: true,
            radius,
            color: Some(color),
            offset: Some(offset),
            spread,
        }
    }

    /// Create an inner shadow effect.
    pub fn inner_shadow(color: Color, offset: Vector2D, radius: f32, spread: f32) -> Self {
        Self {
            effect_type: EffectType::InnerShadow,
            visible: true,
            radius,
            color: Some(color),
            offset: Some(offset),
            spread,
        }
    }

    /// Create a layer blur effect.
    pub fn layer_blur(radius: f32) -> Self {
        Self {
            effect_type: EffectType::LayerBlur,
            visible: true,
            radius,
            color: None,
            offset: None,
            spread: 0.0,
        }
    }

    /// Create a background blur effect.
    pub fn background_blur(radius: f32) -> Self {
        Self {
            effect_type: EffectType::BackgroundBlur,
            visible: true,
            radius,
            color: None,
            offset: None,
            spread: 0.0,
        }
    }
}

impl EffectType {
    /// Decode from Figma type ID.
    pub fn from_figma_id(id: u64) -> Option<Self> {
        match id {
            0 => Some(Self::InnerShadow),
            1 => Some(Self::DropShadow),
            2 => Some(Self::LayerBlur),
            3 => Some(Self::BackgroundBlur),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_shadow() {
        let shadow = Effect::drop_shadow(
            Color::new(0.0, 0.0, 0.0, 0.25),
            Vector2D::new(0.0, 4.0),
            8.0,
            0.0,
        );
        assert_eq!(shadow.effect_type, EffectType::DropShadow);
        assert!(shadow.visible);
        assert!((shadow.radius - 8.0).abs() < 0.001);
        assert!((shadow.offset.unwrap().y - 4.0).abs() < 0.001);
    }

    #[test]
    fn test_inner_shadow() {
        let shadow = Effect::inner_shadow(
            Color::black(),
            Vector2D::new(2.0, 2.0),
            4.0,
            1.0,
        );
        assert_eq!(shadow.effect_type, EffectType::InnerShadow);
        assert!((shadow.spread - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_layer_blur() {
        let blur = Effect::layer_blur(10.0);
        assert_eq!(blur.effect_type, EffectType::LayerBlur);
        assert!(blur.color.is_none());
        assert!(blur.offset.is_none());
        assert!((blur.radius - 10.0).abs() < 0.001);
    }

    #[test]
    fn test_background_blur() {
        let blur = Effect::background_blur(20.0);
        assert_eq!(blur.effect_type, EffectType::BackgroundBlur);
        assert!((blur.radius - 20.0).abs() < 0.001);
    }

    #[test]
    fn test_effect_type_from_id() {
        assert_eq!(EffectType::from_figma_id(0), Some(EffectType::InnerShadow));
        assert_eq!(EffectType::from_figma_id(1), Some(EffectType::DropShadow));
        assert_eq!(EffectType::from_figma_id(2), Some(EffectType::LayerBlur));
        assert_eq!(
            EffectType::from_figma_id(3),
            Some(EffectType::BackgroundBlur)
        );
        assert_eq!(EffectType::from_figma_id(99), None);
    }
}
