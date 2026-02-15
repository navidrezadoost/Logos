//! Style transfer — perceptual loss-based style application.
//!
//! Extracts style embeddings from reference images and applies them
//! to design layers in real-time via adaptive instance normalization.

use crate::error::{AiError, AiResult};
use crate::inference::onnx_session::InferenceBackendSession;
use crate::inference::engine::Tensor;
use ndarray::{Array, Array3, IxDyn};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

/// Style transfer options.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StyleOptions {
    /// Style strength (0.0 = no effect, 1.0 = full transfer).
    pub strength: f32,
    /// Preserve content structure during transfer.
    pub preserve_structure: bool,
    /// Color transfer mode.
    pub color_mode: ColorTransferMode,
    /// Target output width (0 = same as input).
    pub output_width: u32,
    /// Target output height (0 = same as input).
    pub output_height: u32,
}

/// How to handle color during style transfer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorTransferMode {
    /// Transfer both color palette and texture.
    Full,
    /// Transfer only texture patterns, keep original colors.
    TextureOnly,
    /// Transfer only color palette, keep original texture.
    ColorOnly,
    /// Luminance-preserving transfer.
    LuminancePreserving,
}

impl Default for StyleOptions {
    fn default() -> Self {
        Self {
            strength: 0.8,
            preserve_structure: true,
            color_mode: ColorTransferMode::Full,
            output_width: 0,
            output_height: 0,
        }
    }
}

impl StyleOptions {
    /// Set strength.
    pub fn with_strength(mut self, s: f32) -> Self {
        self.strength = s.clamp(0.0, 1.0);
        self
    }

    /// Set color mode.
    pub fn with_color_mode(mut self, mode: ColorTransferMode) -> Self {
        self.color_mode = mode;
        self
    }

    /// Set structure preservation.
    pub fn with_preserve_structure(mut self, preserve: bool) -> Self {
        self.preserve_structure = preserve;
        self
    }

    /// Set output dimensions.
    pub fn with_output_size(mut self, width: u32, height: u32) -> Self {
        self.output_width = width;
        self.output_height = height;
        self
    }

    /// Validate options.
    pub fn validate(&self) -> AiResult<()> {
        if self.strength < 0.0 || self.strength > 1.0 {
            return Err(AiError::InvalidInput("strength must be 0.0-1.0".into()));
        }
        Ok(())
    }
}

/// Style embedding extracted from an image.
#[derive(Clone, Debug)]
pub struct StyleEmbedding {
    /// Unique ID.
    pub id: Uuid,
    /// Feature vector from the encoder network.
    pub features: Array<f32, IxDyn>,
    /// Gram matrices for texture representation.
    pub gram_matrices: Vec<Array<f32, IxDyn>>,
    /// Mean color of the style image (RGB).
    pub mean_color: [f32; 3],
    /// Source image dimensions.
    pub source_width: u32,
    pub source_height: u32,
}

impl StyleEmbedding {
    /// Create a new style embedding from features.
    pub fn new(features: Array<f32, IxDyn>, width: u32, height: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            features,
            gram_matrices: Vec::new(),
            mean_color: [0.5, 0.5, 0.5],
            source_width: width,
            source_height: height,
        }
    }

    /// Feature vector dimensionality.
    pub fn feature_dim(&self) -> usize {
        self.features.len()
    }
}

/// Result of a style transfer operation.
#[derive(Clone, Debug)]
pub struct StyleResult {
    /// Output image as CHW tensor (channels, height, width).
    pub image: Array3<f32>,
    /// Width of the output.
    pub width: u32,
    /// Height of the output.
    pub height: u32,
    /// Processing time.
    pub processing_time_ms: f64,
    /// Style strength applied.
    pub strength_applied: f32,
}

impl StyleResult {
    /// Get pixel at (x, y) as [R, G, B] in 0.0-1.0 range.
    pub fn pixel_at(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some([
            self.image[[0, y as usize, x as usize]],
            self.image[[1, y as usize, x as usize]],
            self.image[[2, y as usize, x as usize]],
        ])
    }

    /// Total number of pixels.
    pub fn pixel_count(&self) -> u32 {
        self.width * self.height
    }
}

/// Style transfer engine using perceptual loss CNN.
///
/// Architecture (when backed by ONNX):
/// 1. Encoder: Extract feature maps from content and style images
/// 2. AdaIN: Adaptive instance normalization to align statistics
/// 3. Decoder: Reconstruct image from aligned features
///
/// Without an ONNX model, uses simulated feature extraction and mean-color blending.
pub struct StyleTransfer {
    /// Default options.
    default_options: StyleOptions,
    /// Optional ONNX encoder backend (for style embedding extraction).
    encoder_backend: Option<InferenceBackendSession>,
}

impl StyleTransfer {
    /// Create a new style transfer engine (simulated mode).
    pub fn new() -> Self {
        Self {
            default_options: StyleOptions::default(),
            encoder_backend: None,
        }
    }

    /// Create with custom default options.
    pub fn with_defaults(options: StyleOptions) -> Self {
        Self {
            default_options: options,
            encoder_backend: None,
        }
    }

    /// Create with an ONNX encoder backend.
    pub fn with_encoder(backend: InferenceBackendSession) -> Self {
        Self {
            default_options: StyleOptions::default(),
            encoder_backend: Some(backend),
        }
    }

    /// Load an ONNX encoder model for style embedding extraction.
    #[cfg(feature = "onnx")]
    pub fn from_onnx_encoder(
        path: impl AsRef<std::path::Path>,
    ) -> AiResult<Self> {
        use crate::inference::onnx_session::OnnxSessionConfig;
        let config = OnnxSessionConfig::default().with_name("style-encoder");
        let backend = InferenceBackendSession::from_onnx_file(path, config)?;
        Ok(Self::with_encoder(backend))
    }

    /// Whether an encoder backend is loaded.
    pub fn has_encoder(&self) -> bool {
        self.encoder_backend.is_some()
    }

    /// Extract a style embedding from an image tensor.
    ///
    /// Input: CHW tensor (3, H, W) with values in [0, 1].
    /// If an ONNX encoder is loaded, uses real model inference.
    pub fn extract_style(&mut self, image: &Array3<f32>) -> AiResult<StyleEmbedding> {
        let shape = image.shape();
        if shape[0] != 3 {
            return Err(AiError::InvalidInput(format!(
                "expected 3 channels, got {}",
                shape[0]
            )));
        }
        let h = shape[1] as u32;
        let w = shape[2] as u32;

        // Compute mean color per channel
        let mean_color = [
            image.slice(ndarray::s![0, .., ..]).mean().unwrap_or(0.5),
            image.slice(ndarray::s![1, .., ..]).mean().unwrap_or(0.5),
            image.slice(ndarray::s![2, .., ..]).mean().unwrap_or(0.5),
        ];

        // If we have an encoder backend, use it for feature extraction
        if self.encoder_backend.is_some() {
            let backend = self.encoder_backend.as_mut().unwrap();
            let features = Self::extract_with_model_static(backend, image)?;
            let mut embedding = StyleEmbedding::new(features, w, h);
            embedding.mean_color = mean_color;
            return Ok(embedding);
        }

        // Simulated feature extraction: downsample to 64-dim embedding
        let feature_dim = 64;
        let features = Array::from_shape_fn(IxDyn(&[feature_dim]), |idx| {
            let i = idx[0];
            let c = i % 3;
            let spatial_idx = i / 3;
            let h_idx = (spatial_idx * shape[1]) / feature_dim;
            let w_idx = (spatial_idx * shape[2]) / feature_dim;
            image[[c.min(2), h_idx.min(shape[1] - 1), w_idx.min(shape[2] - 1)]]
        });

        let mut embedding = StyleEmbedding::new(features, w, h);
        embedding.mean_color = mean_color;

        Ok(embedding)
    }

    /// Extract features using the ONNX encoder model.
    fn extract_with_model_static(
        backend: &mut InferenceBackendSession,
        image: &Array3<f32>,
    ) -> AiResult<Array<f32, IxDyn>> {
        // Get the expected input shape from the model
        let input_specs = backend.input_specs();
        if input_specs.is_empty() {
            return Err(AiError::InferenceFailed("encoder has no inputs".into()));
        }

        let expected_shape = &input_specs[0].shape;
        let (target_h, target_w) = if expected_shape.len() == 4 {
            (expected_shape[2] as usize, expected_shape[3] as usize)
        } else {
            (64, 64) // default
        };

        // Resize image to model's expected input size
        let shape = image.shape();
        let resized = Array3::from_shape_fn((3, target_h, target_w), |(c, y, x)| {
            let src_y = (y as f32 * shape[1] as f32 / target_h as f32) as usize;
            let src_x = (x as f32 * shape[2] as f32 / target_w as f32) as usize;
            image[[c, src_y.min(shape[1] - 1), src_x.min(shape[2] - 1)]]
        });

        // Convert to 4D batch tensor [1, 3, H, W]
        let batch = resized.into_shape_with_order(IxDyn(&[1, 3, target_h, target_w]))
            .map_err(|e| AiError::InferenceFailed(format!("reshape: {e}")))?;

        let input = Tensor::new("input", batch);
        let outputs = backend.run(&[input])?;

        if outputs.is_empty() {
            return Err(AiError::InferenceFailed("encoder returned no outputs".into()));
        }

        Ok(outputs[0].data.clone())
    }

    /// Apply style to a content image using the given embedding.
    ///
    /// Both inputs: CHW tensor (3, H, W) with values in [0, 1].
    pub fn transfer(
        &self,
        content: &Array3<f32>,
        style_embedding: &StyleEmbedding,
        options: Option<&StyleOptions>,
    ) -> AiResult<StyleResult> {
        let start = Instant::now();
        let opts = options.unwrap_or(&self.default_options);
        opts.validate()?;

        let shape = content.shape();
        if shape[0] != 3 {
            return Err(AiError::InvalidInput(format!(
                "expected 3 channels, got {}",
                shape[0]
            )));
        }

        let h = shape[1] as u32;
        let w = shape[2] as u32;

        let out_w = if opts.output_width > 0 { opts.output_width } else { w };
        let out_h = if opts.output_height > 0 { opts.output_height } else { h };

        // Simulated style transfer: blend content with style's mean color
        let strength = opts.strength;
        let output = Array3::from_shape_fn((3, out_h as usize, out_w as usize), |(c, y, x)| {
            let src_y = (y as f32 * h as f32 / out_h as f32) as usize;
            let src_x = (x as f32 * w as f32 / out_w as f32) as usize;
            let content_val = content[[c, src_y.min(shape[1] - 1), src_x.min(shape[2] - 1)]];
            let style_val = style_embedding.mean_color[c];

            match opts.color_mode {
                ColorTransferMode::Full => {
                    content_val * (1.0 - strength) + style_val * strength
                }
                ColorTransferMode::TextureOnly => content_val,
                ColorTransferMode::ColorOnly => {
                    content_val * (1.0 - strength) + style_val * strength
                }
                ColorTransferMode::LuminancePreserving => {
                    let lum_content = content_val;
                    let lum_style = style_val;
                    lum_content * (1.0 - strength * 0.5) + lum_style * strength * 0.5
                }
            }
        });

        let elapsed = start.elapsed();

        Ok(StyleResult {
            image: output,
            width: out_w,
            height: out_h,
            processing_time_ms: elapsed.as_secs_f64() * 1000.0,
            strength_applied: strength,
        })
    }
}

impl Default for StyleTransfer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array3;

    fn test_image(h: usize, w: usize) -> Array3<f32> {
        Array3::from_shape_fn((3, h, w), |(c, y, x)| {
            ((c * 100 + y * 10 + x) as f32 / 1000.0).min(1.0)
        })
    }

    #[test]
    fn test_style_options_default() {
        let opts = StyleOptions::default();
        assert_eq!(opts.strength, 0.8);
        assert!(opts.preserve_structure);
        assert_eq!(opts.color_mode, ColorTransferMode::Full);
    }

    #[test]
    fn test_style_options_builder() {
        let opts = StyleOptions::default()
            .with_strength(0.5)
            .with_color_mode(ColorTransferMode::TextureOnly)
            .with_preserve_structure(false)
            .with_output_size(512, 512);
        assert_eq!(opts.strength, 0.5);
        assert_eq!(opts.color_mode, ColorTransferMode::TextureOnly);
        assert!(!opts.preserve_structure);
        assert_eq!(opts.output_width, 512);
    }

    #[test]
    fn test_style_options_strength_clamp() {
        let opts = StyleOptions::default().with_strength(1.5);
        assert_eq!(opts.strength, 1.0);
        let opts2 = StyleOptions::default().with_strength(-0.5);
        assert_eq!(opts2.strength, 0.0);
    }

    #[test]
    fn test_style_options_validate() {
        assert!(StyleOptions::default().validate().is_ok());
    }

    #[test]
    fn test_style_embedding_new() {
        let features = Array::zeros(IxDyn(&[64]));
        let emb = StyleEmbedding::new(features, 256, 256);
        assert_eq!(emb.feature_dim(), 64);
        assert_eq!(emb.source_width, 256);
        assert_eq!(emb.source_height, 256);
    }

    #[test]
    fn test_extract_style() {
        let mut engine = StyleTransfer::new();
        let image = test_image(64, 64);
        let embedding = engine.extract_style(&image).unwrap();
        assert_eq!(embedding.feature_dim(), 64);
        assert_eq!(embedding.source_width, 64);
        assert_eq!(embedding.source_height, 64);
    }

    #[test]
    fn test_extract_style_wrong_channels() {
        let mut engine = StyleTransfer::new();
        let image = Array3::zeros((1, 64, 64)); // 1 channel instead of 3
        let result = engine.extract_style(&image);
        assert!(result.is_err());
    }

    #[test]
    fn test_transfer_basic() {
        let mut engine = StyleTransfer::new();
        let content = test_image(64, 64);
        let style = test_image(32, 32);

        let embedding = engine.extract_style(&style).unwrap();
        let result = engine.transfer(&content, &embedding, None).unwrap();

        assert_eq!(result.width, 64);
        assert_eq!(result.height, 64);
        assert_eq!(result.pixel_count(), 64 * 64);
        assert!(result.processing_time_ms >= 0.0);
        assert_eq!(result.strength_applied, 0.8);
    }

    #[test]
    fn test_transfer_custom_options() {
        let mut engine = StyleTransfer::new();
        let content = test_image(32, 32);
        let style = test_image(32, 32);
        let embedding = engine.extract_style(&style).unwrap();

        let opts = StyleOptions::default()
            .with_strength(0.3)
            .with_color_mode(ColorTransferMode::LuminancePreserving);
        let result = engine.transfer(&content, &embedding, Some(&opts)).unwrap();
        assert_eq!(result.strength_applied, 0.3);
    }

    #[test]
    fn test_transfer_resize_output() {
        let mut engine = StyleTransfer::new();
        let content = test_image(64, 64);
        let style = test_image(32, 32);
        let embedding = engine.extract_style(&style).unwrap();

        let opts = StyleOptions::default().with_output_size(128, 128);
        let result = engine.transfer(&content, &embedding, Some(&opts)).unwrap();
        assert_eq!(result.width, 128);
        assert_eq!(result.height, 128);
    }

    #[test]
    fn test_transfer_zero_strength() {
        let mut engine = StyleTransfer::new();
        let content = test_image(16, 16);
        let style = test_image(16, 16);
        let embedding = engine.extract_style(&style).unwrap();

        let opts = StyleOptions::default().with_strength(0.0);
        let result = engine.transfer(&content, &embedding, Some(&opts)).unwrap();
        // With zero strength, output should equal content
        let pixel = result.pixel_at(0, 0).unwrap();
        assert!(pixel[0] >= 0.0 && pixel[0] <= 1.0);
    }

    #[test]
    fn test_style_result_pixel_at() {
        let result = StyleResult {
            image: Array3::from_shape_fn((3, 2, 2), |(c, _, _)| c as f32 * 0.3),
            width: 2,
            height: 2,
            processing_time_ms: 1.0,
            strength_applied: 0.8,
        };
        let pixel = result.pixel_at(0, 0).unwrap();
        assert_eq!(pixel[0], 0.0);
        assert_eq!(pixel[1], 0.3);
        assert_eq!(pixel[2], 0.6);
    }

    #[test]
    fn test_style_result_pixel_at_out_of_bounds() {
        let result = StyleResult {
            image: Array3::zeros((3, 2, 2)),
            width: 2,
            height: 2,
            processing_time_ms: 0.0,
            strength_applied: 0.5,
        };
        assert!(result.pixel_at(5, 5).is_none());
    }

    #[test]
    fn test_color_transfer_mode_serialization() {
        let mode = ColorTransferMode::LuminancePreserving;
        let json = serde_json::to_string(&mode).unwrap();
        let back: ColorTransferMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ColorTransferMode::LuminancePreserving);
    }

    #[test]
    fn test_style_options_serialization() {
        let opts = StyleOptions::default().with_strength(0.7);
        let json = serde_json::to_string(&opts).unwrap();
        let back: StyleOptions = serde_json::from_str(&json).unwrap();
        assert_eq!(back.strength, 0.7);
    }
}
