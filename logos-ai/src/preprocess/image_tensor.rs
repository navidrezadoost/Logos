//! Image tensor preprocessing utilities.
//!
//! Converts raw pixel data into normalized CHW tensors for inference.

use crate::error::{AiError, AiResult};
use ndarray::{Array, Array3, IxDyn};

/// Image tensor with preprocessing capabilities.
///
/// Wraps a CHW float tensor (channels, height, width) with normalization
/// and conversion utilities.
#[derive(Clone, Debug)]
pub struct ImageTensor {
    /// Image data as CHW (3, H, W).
    data: Array3<f32>,
    /// Width.
    width: u32,
    /// Height.
    height: u32,
}

impl ImageTensor {
    /// Create from a CHW array (3, H, W) with values in [0, 1].
    pub fn from_chw(data: Array3<f32>) -> AiResult<Self> {
        let shape = data.shape();
        if shape[0] != 3 {
            return Err(AiError::InvalidInput(format!(
                "expected 3 channels, got {}",
                shape[0]
            )));
        }
        Ok(Self {
            width: shape[2] as u32,
            height: shape[1] as u32,
            data,
        })
    }

    /// Create from raw RGB bytes (H×W×3, row-major, 0-255).
    pub fn from_rgb_bytes(bytes: &[u8], width: u32, height: u32) -> AiResult<Self> {
        let expected = (width * height * 3) as usize;
        if bytes.len() != expected {
            return Err(AiError::InvalidInput(format!(
                "expected {} bytes, got {}",
                expected,
                bytes.len()
            )));
        }

        let h = height as usize;
        let w = width as usize;
        let data = Array3::from_shape_fn((3, h, w), |(c, y, x)| {
            bytes[(y * w + x) * 3 + c] as f32 / 255.0
        });

        Ok(Self {
            data,
            width,
            height,
        })
    }

    /// Create from RGBA bytes (H×W×4, row-major, 0-255).
    pub fn from_rgba_bytes(bytes: &[u8], width: u32, height: u32) -> AiResult<Self> {
        let expected = (width * height * 4) as usize;
        if bytes.len() != expected {
            return Err(AiError::InvalidInput(format!(
                "expected {} bytes, got {}",
                expected,
                bytes.len()
            )));
        }

        let h = height as usize;
        let w = width as usize;
        let data = Array3::from_shape_fn((3, h, w), |(c, y, x)| {
            bytes[(y * w + x) * 4 + c] as f32 / 255.0
        });

        Ok(Self {
            data,
            width,
            height,
        })
    }

    /// Create a blank (black) image.
    pub fn blank(width: u32, height: u32) -> Self {
        Self {
            data: Array3::zeros((3, height as usize, width as usize)),
            width,
            height,
        }
    }

    /// Width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Get the underlying CHW data.
    pub fn data(&self) -> &Array3<f32> {
        &self.data
    }

    /// Consume and return the underlying CHW data.
    pub fn into_data(self) -> Array3<f32> {
        self.data
    }

    /// Normalize using ImageNet mean and std.
    ///
    /// Standard normalization: `(pixel - mean) / std`
    /// - mean: [0.485, 0.456, 0.406]
    /// - std:  [0.229, 0.224, 0.225]
    pub fn normalize_imagenet(&self) -> Array3<f32> {
        let mean = [0.485f32, 0.456, 0.406];
        let std = [0.229f32, 0.224, 0.225];
        self.normalize(&mean, &std)
    }

    /// Normalize with custom mean and std per channel.
    pub fn normalize(&self, mean: &[f32; 3], std: &[f32; 3]) -> Array3<f32> {
        Array3::from_shape_fn(self.data.raw_dim(), |(c, y, x)| {
            (self.data[[c, y, x]] - mean[c]) / std[c]
        })
    }

    /// Scale pixel values to [-1, 1] range (common for diffusion models).
    pub fn normalize_symmetric(&self) -> Array3<f32> {
        self.data.mapv(|v| v * 2.0 - 1.0)
    }

    /// Convert to NCHW batch tensor (1, 3, H, W) for model input.
    pub fn to_batch_tensor(&self) -> Array<f32, IxDyn> {
        let shape = IxDyn(&[1, 3, self.height as usize, self.width as usize]);
        Array::from_shape_fn(shape, |idx| {
            self.data[[idx[1], idx[2], idx[3]]]
        })
    }

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

    /// Convert back to RGB bytes (H×W×3, row-major, 0-255).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> ImageTensor {
        let data = Array3::from_shape_fn((3, 4, 4), |(c, y, x)| {
            (c as f32 * 0.1 + y as f32 * 0.05 + x as f32 * 0.02).min(1.0)
        });
        ImageTensor::from_chw(data).unwrap()
    }

    #[test]
    fn test_from_chw() {
        let img = test_image();
        assert_eq!(img.width(), 4);
        assert_eq!(img.height(), 4);
    }

    #[test]
    fn test_from_chw_wrong_channels() {
        let data = Array3::zeros((1, 4, 4));
        assert!(ImageTensor::from_chw(data).is_err());
    }

    #[test]
    fn test_from_rgb_bytes() {
        let bytes = vec![128u8; 2 * 2 * 3]; // 2x2 gray image
        let img = ImageTensor::from_rgb_bytes(&bytes, 2, 2).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        let pixel = img.pixel_at(0, 0).unwrap();
        assert!((pixel[0] - 128.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn test_from_rgb_bytes_wrong_size() {
        let bytes = vec![0u8; 10];
        assert!(ImageTensor::from_rgb_bytes(&bytes, 2, 2).is_err());
    }

    #[test]
    fn test_from_rgba_bytes() {
        let bytes = vec![100u8; 2 * 2 * 4]; // 2x2 RGBA
        let img = ImageTensor::from_rgba_bytes(&bytes, 2, 2).unwrap();
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
    }

    #[test]
    fn test_blank() {
        let img = ImageTensor::blank(32, 32);
        assert_eq!(img.width(), 32);
        assert_eq!(img.height(), 32);
        let pixel = img.pixel_at(0, 0).unwrap();
        assert_eq!(pixel, [0.0, 0.0, 0.0]);
    }

    #[test]
    fn test_normalize_imagenet() {
        let img = test_image();
        let normalized = img.normalize_imagenet();
        assert_eq!(normalized.shape(), &[3, 4, 4]);
        // Check first pixel channel 0: (0.0 - 0.485) / 0.229 ≈ -2.118
        let val = normalized[[0, 0, 0]];
        assert!((val - (-0.485 / 0.229)).abs() < 0.01);
    }

    #[test]
    fn test_normalize_symmetric() {
        let data = Array3::from_elem((3, 2, 2), 0.5f32);
        let img = ImageTensor::from_chw(data).unwrap();
        let sym = img.normalize_symmetric();
        assert!((sym[[0, 0, 0]] - 0.0).abs() < 0.001); // 0.5 * 2 - 1 = 0
    }

    #[test]
    fn test_to_batch_tensor() {
        let img = test_image();
        let batch = img.to_batch_tensor();
        assert_eq!(batch.shape(), &[1, 3, 4, 4]);
    }

    #[test]
    fn test_pixel_at() {
        let img = test_image();
        let pixel = img.pixel_at(0, 0).unwrap();
        assert!(pixel[0] >= 0.0);
        assert!(img.pixel_at(100, 100).is_none());
    }

    #[test]
    fn test_to_rgb_bytes_roundtrip() {
        let bytes_in = vec![64u8, 128, 192, 64, 128, 192, 64, 128, 192, 64, 128, 192];
        let img = ImageTensor::from_rgb_bytes(&bytes_in, 2, 2).unwrap();
        let bytes_out = img.to_rgb_bytes();
        assert_eq!(bytes_in, bytes_out);
    }

    #[test]
    fn test_into_data() {
        let img = test_image();
        let data = img.into_data();
        assert_eq!(data.shape(), &[3, 4, 4]);
    }
}
