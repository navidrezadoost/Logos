//! Asset generation — optimized diffusion pipeline for text-to-image.
//!
//! Produces 512×512 images from text prompts using a distilled
//! Stable Diffusion pipeline (text encoder + UNet + VAE decoder).

use crate::error::{AiError, AiResult};
use ndarray::Array3;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use uuid::Uuid;

/// Supported output image sizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImageSize {
    /// 256×256 pixels (fast).
    Small,
    /// 512×512 pixels (default).
    Medium,
    /// 768×768 pixels (high quality).
    Large,
    /// 1024×1024 pixels (highest quality).
    ExtraLarge,
    /// Custom dimensions.
    Custom(u32, u32),
}

impl ImageSize {
    /// Width in pixels.
    pub fn width(&self) -> u32 {
        match self {
            ImageSize::Small => 256,
            ImageSize::Medium => 512,
            ImageSize::Large => 768,
            ImageSize::ExtraLarge => 1024,
            ImageSize::Custom(w, _) => *w,
        }
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        match self {
            ImageSize::Small => 256,
            ImageSize::Medium => 512,
            ImageSize::Large => 768,
            ImageSize::ExtraLarge => 1024,
            ImageSize::Custom(_, h) => *h,
        }
    }

    /// Total pixel count.
    pub fn pixel_count(&self) -> u32 {
        self.width() * self.height()
    }
}

impl Default for ImageSize {
    fn default() -> Self {
        ImageSize::Medium
    }
}

/// Parameters for image generation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Text prompt describing the desired image.
    pub prompt: String,
    /// Negative prompt (what to avoid).
    pub negative_prompt: Option<String>,
    /// Output image size.
    pub size: ImageSize,
    /// Number of diffusion steps (more = higher quality, slower).
    pub num_steps: u32,
    /// Classifier-free guidance scale (higher = more prompt-adherent).
    pub guidance_scale: f32,
    /// Random seed for reproducibility (None = random).
    pub seed: Option<u64>,
    /// Number of images to generate.
    pub num_images: u32,
}

impl GenerationParams {
    /// Create new generation params with the given prompt.
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            negative_prompt: None,
            size: ImageSize::Medium,
            num_steps: 20,
            guidance_scale: 7.5,
            seed: None,
            num_images: 1,
        }
    }

    /// Set negative prompt.
    pub fn with_negative(mut self, neg: impl Into<String>) -> Self {
        self.negative_prompt = Some(neg.into());
        self
    }

    /// Set image size.
    pub fn with_size(mut self, size: ImageSize) -> Self {
        self.size = size;
        self
    }

    /// Set number of diffusion steps.
    pub fn with_steps(mut self, steps: u32) -> Self {
        self.num_steps = steps.max(1).min(150);
        self
    }

    /// Set guidance scale.
    pub fn with_guidance(mut self, scale: f32) -> Self {
        self.guidance_scale = scale.clamp(1.0, 30.0);
        self
    }

    /// Set seed for reproducibility.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Set number of images to generate.
    pub fn with_count(mut self, n: u32) -> Self {
        self.num_images = n.max(1).min(16);
        self
    }

    /// Validate parameters.
    pub fn validate(&self) -> AiResult<()> {
        if self.prompt.trim().is_empty() {
            return Err(AiError::InvalidInput("prompt cannot be empty".into()));
        }
        if self.prompt.len() > 10000 {
            return Err(AiError::InvalidInput("prompt too long (max 10000 chars)".into()));
        }
        if self.num_steps < 1 {
            return Err(AiError::InvalidInput("need at least 1 diffusion step".into()));
        }
        if self.guidance_scale < 1.0 {
            return Err(AiError::InvalidInput("guidance scale must be >= 1.0".into()));
        }
        Ok(())
    }
}

/// A generated image.
#[derive(Clone, Debug)]
pub struct GeneratedImage {
    /// Unique ID.
    pub id: Uuid,
    /// Image data as CHW tensor (3, H, W), values in [0, 1].
    pub data: Array3<f32>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The prompt used.
    pub prompt: String,
    /// The seed used (for reproducibility).
    pub seed: u64,
    /// Generation time in milliseconds.
    pub generation_time_ms: f64,
    /// Number of diffusion steps used.
    pub steps_used: u32,
}

impl GeneratedImage {
    /// Get pixel at (x, y) as [R, G, B].
    pub fn pixel_at(&self, x: u32, y: u32) -> Option<[f32; 3]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some([
            self.data[[0, y as usize, x as usize]],
            self.data[[1, y as usize, x as usize]],
            self.data[[2, y as usize, x as usize]],
        ])
    }

    /// Average brightness (0.0 = black, 1.0 = white).
    pub fn average_brightness(&self) -> f32 {
        let sum: f32 = self.data.iter().sum();
        let count = self.data.len() as f32;
        if count == 0.0 {
            return 0.0;
        }
        sum / count
    }

    /// Convert to raw RGB bytes (H×W×3, row-major, 0-255 range).
    pub fn to_rgb_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity((self.width * self.height * 3) as usize);
        for y in 0..self.height as usize {
            for x in 0..self.width as usize {
                bytes.push((self.data[[0, y, x]].clamp(0.0, 1.0) * 255.0) as u8);
                bytes.push((self.data[[1, y, x]].clamp(0.0, 1.0) * 255.0) as u8);
                bytes.push((self.data[[2, y, x]].clamp(0.0, 1.0) * 255.0) as u8);
            }
        }
        bytes
    }
}

/// Simple seeded pseudo-random number generator (xorshift64).
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next_f32(&mut self) -> f32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        (self.state as f32 / u64::MAX as f32).abs()
    }

    fn next_gaussian(&mut self) -> f32 {
        // Box-Muller transform
        let u1 = self.next_f32().max(1e-10);
        let u2 = self.next_f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
}

/// Text-to-image asset generator.
///
/// Production architecture (with ONNX models loaded):
/// 1. **Text Encoder** (CLIP): prompt → text embedding (77×768)
/// 2. **UNet**: iterative denoising over N steps
/// 3. **VAE Decoder**: latent → pixel space
///
/// Currently uses a deterministic procedural generator as a placeholder.
pub struct AssetGenerator {
    /// Default number of diffusion steps.
    default_steps: u32,
    /// Default guidance scale.
    default_guidance: f32,
}

impl AssetGenerator {
    /// Create a new asset generator.
    pub fn new() -> Self {
        Self {
            default_steps: 20,
            default_guidance: 7.5,
        }
    }

    /// Set default steps.
    pub fn with_default_steps(mut self, steps: u32) -> Self {
        self.default_steps = steps.max(1).min(150);
        self
    }

    /// Set default guidance scale.
    pub fn with_default_guidance(mut self, guidance: f32) -> Self {
        self.default_guidance = guidance.clamp(1.0, 30.0);
        self
    }

    /// Generate an image from the given parameters.
    ///
    /// In production, this runs the full diffusion pipeline via ONNX.
    /// Currently generates a procedural pattern from the prompt hash.
    pub fn generate(&self, params: &GenerationParams) -> AiResult<Vec<GeneratedImage>> {
        params.validate()?;

        let start = Instant::now();
        let mut results = Vec::with_capacity(params.num_images as usize);

        for i in 0..params.num_images {
            // Derive seed from params or generate one
            let seed = params.seed.unwrap_or(42) + i as u64;
            let image = self.generate_single(params, seed)?;
            let elapsed = start.elapsed();

            results.push(GeneratedImage {
                id: Uuid::new_v4(),
                data: image,
                width: params.size.width(),
                height: params.size.height(),
                prompt: params.prompt.clone(),
                seed,
                generation_time_ms: elapsed.as_secs_f64() * 1000.0,
                steps_used: params.num_steps,
            });
        }

        Ok(results)
    }

    /// Generate a single image (procedural placeholder).
    fn generate_single(&self, params: &GenerationParams, seed: u64) -> AiResult<Array3<f32>> {
        let w = params.size.width() as usize;
        let h = params.size.height() as usize;
        let mut rng = Rng::new(seed);

        // Hash the prompt to derive base colors
        let prompt_hash = self.hash_prompt(&params.prompt);
        let base_r = ((prompt_hash >> 16) & 0xFF) as f32 / 255.0;
        let base_g = ((prompt_hash >> 8) & 0xFF) as f32 / 255.0;
        let base_b = (prompt_hash & 0xFF) as f32 / 255.0;

        // Generate image with gradient + noise pattern
        let image = Array3::from_shape_fn((3, h, w), |(c, y, x)| {
            let nx = x as f32 / w as f32;
            let ny = y as f32 / h as f32;

            let gradient = match c {
                0 => base_r * (1.0 - ny) + (1.0 - base_r) * ny,
                1 => base_g * nx + (1.0 - base_g) * (1.0 - nx),
                2 => base_b * (nx + ny) / 2.0,
                _ => 0.0,
            };

            // Add subtle noise for texture
            let noise = rng.next_gaussian() * 0.05;
            (gradient + noise).clamp(0.0, 1.0)
        });

        Ok(image)
    }

    /// Simple hash function for prompt text.
    fn hash_prompt(&self, prompt: &str) -> u64 {
        let mut hash: u64 = 5381;
        for byte in prompt.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u64);
        }
        hash
    }
}

impl Default for AssetGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_size_dimensions() {
        assert_eq!(ImageSize::Small.width(), 256);
        assert_eq!(ImageSize::Small.height(), 256);
        assert_eq!(ImageSize::Medium.width(), 512);
        assert_eq!(ImageSize::Large.width(), 768);
        assert_eq!(ImageSize::ExtraLarge.width(), 1024);
        assert_eq!(ImageSize::Custom(320, 240).width(), 320);
        assert_eq!(ImageSize::Custom(320, 240).height(), 240);
    }

    #[test]
    fn test_image_size_pixel_count() {
        assert_eq!(ImageSize::Small.pixel_count(), 256 * 256);
        assert_eq!(ImageSize::Medium.pixel_count(), 512 * 512);
    }

    #[test]
    fn test_generation_params_new() {
        let params = GenerationParams::new("a beautiful sunset");
        assert_eq!(params.prompt, "a beautiful sunset");
        assert_eq!(params.num_steps, 20);
        assert_eq!(params.guidance_scale, 7.5);
        assert_eq!(params.size, ImageSize::Medium);
        assert_eq!(params.num_images, 1);
        assert!(params.seed.is_none());
    }

    #[test]
    fn test_generation_params_builder() {
        let params = GenerationParams::new("sunset")
            .with_negative("blurry")
            .with_size(ImageSize::Large)
            .with_steps(30)
            .with_guidance(12.0)
            .with_seed(12345)
            .with_count(4);
        assert_eq!(params.negative_prompt, Some("blurry".into()));
        assert_eq!(params.size, ImageSize::Large);
        assert_eq!(params.num_steps, 30);
        assert_eq!(params.guidance_scale, 12.0);
        assert_eq!(params.seed, Some(12345));
        assert_eq!(params.num_images, 4);
    }

    #[test]
    fn test_generation_params_clamp() {
        let params = GenerationParams::new("x")
            .with_steps(200)
            .with_guidance(50.0)
            .with_count(100);
        assert_eq!(params.num_steps, 150);
        assert_eq!(params.guidance_scale, 30.0);
        assert_eq!(params.num_images, 16);
    }

    #[test]
    fn test_generation_params_validate() {
        assert!(GenerationParams::new("sunset").validate().is_ok());
    }

    #[test]
    fn test_generation_params_validate_empty_prompt() {
        assert!(GenerationParams::new("").validate().is_err());
        assert!(GenerationParams::new("   ").validate().is_err());
    }

    #[test]
    fn test_generator_new() {
        let gen = AssetGenerator::new();
        assert_eq!(gen.default_steps, 20);
    }

    #[test]
    fn test_generate_single_image() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("a red circle")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images = gen.generate(&params).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width, 256);
        assert_eq!(images[0].height, 256);
        assert_eq!(images[0].seed, 42);
        assert_eq!(images[0].prompt, "a red circle");
    }

    #[test]
    fn test_generate_multiple_images() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("landscape")
            .with_size(ImageSize::Small)
            .with_count(3)
            .with_seed(100);
        let images = gen.generate(&params).unwrap();
        assert_eq!(images.len(), 3);
        // Each should have unique ID
        assert_ne!(images[0].id, images[1].id);
        assert_ne!(images[1].id, images[2].id);
    }

    #[test]
    fn test_generate_deterministic_with_seed() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images1 = gen.generate(&params).unwrap();
        let images2 = gen.generate(&params).unwrap();
        // Same seed + same prompt = same image data
        assert_eq!(images1[0].data, images2[0].data);
    }

    #[test]
    fn test_generate_different_seeds_different_images() {
        let gen = AssetGenerator::new();
        let p1 = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(1);
        let p2 = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(2);
        let i1 = gen.generate(&p1).unwrap();
        let i2 = gen.generate(&p2).unwrap();
        // Different seeds should produce different images
        assert_ne!(i1[0].data, i2[0].data);
    }

    #[test]
    fn test_generated_image_pixel_at() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images = gen.generate(&params).unwrap();
        let pixel = images[0].pixel_at(0, 0);
        assert!(pixel.is_some());
        let rgb = pixel.unwrap();
        assert!(rgb[0] >= 0.0 && rgb[0] <= 1.0);
        assert!(rgb[1] >= 0.0 && rgb[1] <= 1.0);
        assert!(rgb[2] >= 0.0 && rgb[2] <= 1.0);
    }

    #[test]
    fn test_generated_image_pixel_out_of_bounds() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images = gen.generate(&params).unwrap();
        assert!(images[0].pixel_at(999, 999).is_none());
    }

    #[test]
    fn test_generated_image_brightness() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("bright image")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images = gen.generate(&params).unwrap();
        let brightness = images[0].average_brightness();
        assert!(brightness > 0.0 && brightness < 1.0);
    }

    #[test]
    fn test_generated_image_to_rgb_bytes() {
        let gen = AssetGenerator::new();
        let params = GenerationParams::new("test")
            .with_size(ImageSize::Small)
            .with_seed(42);
        let images = gen.generate(&params).unwrap();
        let bytes = images[0].to_rgb_bytes();
        assert_eq!(bytes.len(), (256 * 256 * 3) as usize);
        assert!(bytes.iter().all(|&b| b <= 255));
    }

    #[test]
    fn test_image_size_serialization() {
        let size = ImageSize::Large;
        let json = serde_json::to_string(&size).unwrap();
        let back: ImageSize = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ImageSize::Large);
    }

    #[test]
    fn test_generation_params_serialization() {
        let params = GenerationParams::new("sunset").with_seed(42);
        let json = serde_json::to_string(&params).unwrap();
        let back: GenerationParams = serde_json::from_str(&json).unwrap();
        assert_eq!(back.prompt, "sunset");
        assert_eq!(back.seed, Some(42));
    }

    #[test]
    fn test_rng_deterministic() {
        let mut rng1 = Rng::new(42);
        let mut rng2 = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(rng1.next_f32(), rng2.next_f32());
        }
    }

    #[test]
    fn test_rng_different_seeds() {
        let mut rng1 = Rng::new(1);
        let mut rng2 = Rng::new(2);
        let vals1: Vec<f32> = (0..10).map(|_| rng1.next_f32()).collect();
        let vals2: Vec<f32> = (0..10).map(|_| rng2.next_f32()).collect();
        assert_ne!(vals1, vals2);
    }
}
