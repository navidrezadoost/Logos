//! PNG / raster export — software rasterizer for basic shapes.
//!
//! Provides a lightweight, zero-dependency raster pipeline that renders
//! Logos layers into an RGBA pixel buffer and encodes the result to PNG.
//!
//! Design goals:
//! - No GPU or external imaging crate required
//! - Correct anti-aliased output for basic geometry
//! - Configurable DPI and scale for retina/HiDPI export

use crate::color::Color;
use crate::{ExportError, ExportLayerData, ExportPage};
use logos_core::Layer;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// Configuration for raster export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RasterConfig {
    /// Dots per inch — affects metadata, not pixel count (use `scale`).
    pub dpi: f32,
    /// Scale multiplier applied to page dimensions.
    pub scale: f32,
    /// Enable simple box anti-aliasing (2×2 SSAA).
    pub anti_alias: bool,
    /// Bits per channel (8 or 16).
    pub bits_per_channel: u8,
}

impl Default for RasterConfig {
    fn default() -> Self {
        Self {
            dpi: 72.0,
            scale: 1.0,
            anti_alias: false,
            bits_per_channel: 8,
        }
    }
}

impl RasterConfig {
    pub fn retina() -> Self {
        Self { dpi: 144.0, scale: 2.0, ..Default::default() }
    }

    pub fn print_300dpi() -> Self {
        Self { dpi: 300.0, scale: 300.0 / 72.0, anti_alias: true, ..Default::default() }
    }

    /// Resolved pixel width for a page.
    pub fn pixel_width(&self, page: &ExportPage) -> u32 {
        (page.width * self.scale).ceil() as u32
    }

    /// Resolved pixel height for a page.
    pub fn pixel_height(&self, page: &ExportPage) -> u32 {
        (page.height * self.scale).ceil() as u32
    }
}

/// RGBA pixel buffer.
#[derive(Debug, Clone)]
pub struct RasterBuffer {
    pub width: u32,
    pub height: u32,
    /// Row-major RGBA pixels.
    pub pixels: Vec<u8>,
}

impl RasterBuffer {
    /// Create a new buffer filled with a background color.
    pub fn new(width: u32, height: u32, bg: Color) -> Self {
        let [r, g, b, a] = bg.to_u8();
        let count = (width as usize) * (height as usize);
        let mut pixels = Vec::with_capacity(count * 4);
        for _ in 0..count {
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }
        Self { width, height, pixels }
    }

    /// Create a transparent buffer.
    pub fn transparent(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0u8; (width as usize) * (height as usize) * 4],
        }
    }

    /// Set a pixel at (x, y) with alpha blending.
    pub fn blend_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let dst = Color::from_u8(
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        );
        let blended = color.blend_over(&dst);
        let [r, g, b, a] = blended.to_u8();
        self.pixels[idx] = r;
        self.pixels[idx + 1] = g;
        self.pixels[idx + 2] = b;
        self.pixels[idx + 3] = a;
    }

    /// Set a pixel directly (no blending).
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width || y >= self.height {
            return;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        let [r, g, b, a] = color.to_u8();
        self.pixels[idx] = r;
        self.pixels[idx + 1] = g;
        self.pixels[idx + 2] = b;
        self.pixels[idx + 3] = a;
    }

    /// Read a pixel at (x, y).
    pub fn get_pixel(&self, x: u32, y: u32) -> Color {
        if x >= self.width || y >= self.height {
            return Color::TRANSPARENT;
        }
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        Color::from_u8(
            self.pixels[idx],
            self.pixels[idx + 1],
            self.pixels[idx + 2],
            self.pixels[idx + 3],
        )
    }

    /// Fill a rectangle region.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color) {
        let x0 = x.floor().max(0.0) as u32;
        let y0 = y.floor().max(0.0) as u32;
        let x1 = (x + w).ceil().min(self.width as f32) as u32;
        let y1 = (y + h).ceil().min(self.height as f32) as u32;
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px, py, color);
            }
        }
    }

    /// Fill an axis-aligned ellipse inscribed in the given rect.
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Color) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let x0 = (cx - rx).floor().max(0.0) as u32;
        let y0 = (cy - ry).floor().max(0.0) as u32;
        let x1 = (cx + rx).ceil().min(self.width as f32) as u32;
        let y1 = (cy + ry).ceil().min(self.height as f32) as u32;
        for py in y0..y1 {
            for px in x0..x1 {
                let dx = (px as f32 + 0.5 - cx) / rx;
                let dy = (py as f32 + 0.5 - cy) / ry;
                if dx * dx + dy * dy <= 1.0 {
                    self.blend_pixel(px, py, color);
                }
            }
        }
    }

    /// Draw a 1-pixel horizontal line.
    pub fn draw_hline(&mut self, x0: u32, x1: u32, y: u32, color: Color) {
        if y >= self.height {
            return;
        }
        let start = x0.min(self.width);
        let end = x1.min(self.width);
        for x in start..end {
            self.blend_pixel(x, y, color);
        }
    }
}

// ── Minimal PNG encoder ───────────────────────────────────────────

/// Encode an RGBA buffer as a PNG to a writer.
///
/// This is a minimal encoder compliant with the PNG specification (RFC 2083).
/// It writes: PNG signature, IHDR, IDAT (uncompressed deflate), IEND.
pub fn encode_png<W: Write>(buf: &RasterBuffer, writer: &mut W) -> Result<(), ExportError> {
    if buf.width == 0 || buf.height == 0 {
        return Err(ExportError::InvalidDimensions(buf.width as f32, buf.height as f32));
    }

    // PNG signature
    writer.write_all(&[137, 80, 78, 71, 13, 10, 26, 10])
        .map_err(ExportError::Io)?;

    // IHDR
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&buf.width.to_be_bytes());
    ihdr.extend_from_slice(&buf.height.to_be_bytes());
    ihdr.push(8); // bit depth
    ihdr.push(6); // color type: RGBA
    ihdr.push(0); // compression
    ihdr.push(0); // filter
    ihdr.push(0); // interlace
    write_png_chunk(writer, b"IHDR", &ihdr)?;

    // IDAT — build raw image data (filter byte 0 = None per row)
    let row_bytes = (buf.width as usize) * 4 + 1; // +1 for filter
    let raw_len = row_bytes * (buf.height as usize);

    // We use uncompressed deflate blocks (stored blocks).
    // deflate: blocks of ≤65535 bytes each.
    let mut raw = Vec::with_capacity(raw_len);
    for y in 0..buf.height {
        raw.push(0); // filter: None
        let start = (y as usize) * (buf.width as usize) * 4;
        let end = start + (buf.width as usize) * 4;
        raw.extend_from_slice(&buf.pixels[start..end]);
    }

    // Wrap in zlib (CMF + FLG + deflate blocks + Adler32)
    let mut zlib = Vec::new();
    zlib.push(0x78); // CMF: deflate, window 32K
    zlib.push(0x01); // FLG: check bits (0x7801 mod 31 == 0)

    // Write stored deflate blocks
    let mut offset = 0;
    while offset < raw.len() {
        let remaining = raw.len() - offset;
        let block_size = remaining.min(65535);
        let is_last = offset + block_size >= raw.len();
        zlib.push(if is_last { 1 } else { 0 }); // BFINAL
        let len = block_size as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes()); // NLEN
        zlib.extend_from_slice(&raw[offset..offset + block_size]);
        offset += block_size;
    }

    // Adler-32
    let adler = adler32(&raw);
    zlib.extend_from_slice(&adler.to_be_bytes());

    write_png_chunk(writer, b"IDAT", &zlib)?;

    // IEND
    write_png_chunk(writer, b"IEND", &[])?;

    Ok(())
}

fn write_png_chunk<W: Write>(w: &mut W, chunk_type: &[u8; 4], data: &[u8]) -> Result<(), ExportError> {
    let len = data.len() as u32;
    w.write_all(&len.to_be_bytes()).map_err(ExportError::Io)?;
    w.write_all(chunk_type).map_err(ExportError::Io)?;
    w.write_all(data).map_err(ExportError::Io)?;

    let mut crc_data = Vec::with_capacity(4 + data.len());
    crc_data.extend_from_slice(chunk_type);
    crc_data.extend_from_slice(data);
    let crc = crc32(&crc_data);
    w.write_all(&crc.to_be_bytes()).map_err(ExportError::Io)?;
    Ok(())
}

/// CRC-32 (ISO 3309 / PNG Annex D).
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// Adler-32 checksum.
fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &byte in data {
        a = (a + byte as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

// ── PngExporter ──────────────────────────────────────────────────

/// High-level PNG exporter that renders layers to a raster buffer.
pub struct PngExporter {
    pub page: ExportPage,
    pub config: RasterConfig,
}

impl PngExporter {
    pub fn new(page: ExportPage) -> Self {
        Self {
            page,
            config: RasterConfig::default(),
        }
    }

    pub fn with_config(mut self, config: RasterConfig) -> Self {
        self.config = config;
        self
    }

    /// Rasterize layers into a pixel buffer.
    pub fn rasterize(&self, layers: &[ExportLayerData<'_>]) -> RasterBuffer {
        let w = self.config.pixel_width(&self.page);
        let h = self.config.pixel_height(&self.page);
        let bg = self.page.background
            .map(Color::from)
            .unwrap_or(Color::TRANSPARENT);
        let mut buf = RasterBuffer::new(w, h, bg);
        let scale = self.config.scale;

        for data in layers {
            let color = default_raster_color(&data.layer);
            let x = data.x * scale;
            let y = data.y * scale;
            let lw = data.width * scale;
            let lh = data.height * scale;

            match &data.layer {
                Layer::Rect(_) | Layer::Frame(_) | Layer::Artboard(_) | Layer::Drawer(_) => {
                    buf.fill_rect(x, y, lw, lh, color);
                }
                Layer::Ellipse(_) => {
                    let cx = x + lw / 2.0;
                    let cy = y + lh / 2.0;
                    buf.fill_ellipse(cx, cy, lw / 2.0, lh / 2.0, color);
                }
                Layer::Text(_) => {
                    // Text rendered as a filled rect placeholder
                    buf.fill_rect(x, y, lw, lh, color);
                }
                Layer::Path(_) => {
                    // Path simplified as bounding-box fill
                    buf.fill_rect(x, y, lw, lh, color);
                }
                Layer::Section(_) => {
                    // Sections are structural — skip
                }
                Layer::Line(_) | Layer::Polygon(_) | Layer::Star(_)
                | Layer::BooleanGroup(_) | Layer::VectorNetwork(_) => {
                    buf.fill_rect(x, y, lw, lh, color);
                }
            }
        }
        buf
    }

    /// Export layers to PNG bytes.
    pub fn export_to_bytes(&self, layers: &[ExportLayerData<'_>]) -> Result<Vec<u8>, ExportError> {
        let buf = self.rasterize(layers);
        let mut out = Vec::new();
        encode_png(&buf, &mut out)?;
        Ok(out)
    }

    /// Export layers to a writer.
    pub fn export_to_writer<W: Write>(
        &self,
        layers: &[ExportLayerData<'_>],
        writer: &mut W,
    ) -> Result<(), ExportError> {
        let buf = self.rasterize(layers);
        encode_png(&buf, writer)
    }
}

/// Default fill color per layer type (for raster).
fn default_raster_color(layer: &Layer) -> Color {
    match layer {
        Layer::Rect(_) => Color::from([0.75, 0.85, 0.95, 1.0]),
        Layer::Ellipse(_) => Color::from([0.95, 0.80, 0.75, 1.0]),
        Layer::Text(_) => Color::from([0.2, 0.2, 0.2, 1.0]),
        Layer::Frame(_) => Color::from([0.9, 0.9, 0.9, 0.8]),
        Layer::Path(_) => Color::from([0.3, 0.3, 0.8, 1.0]),
        Layer::Artboard(_) | Layer::Drawer(_) => Color::from([0.95, 0.95, 0.95, 0.9]),
        Layer::Section(_) => Color::TRANSPARENT,
        Layer::Line(_) => Color::from([0.2, 0.2, 0.2, 1.0]),
        Layer::Polygon(_) => Color::from([0.4, 0.2, 0.8, 1.0]),
        Layer::Star(_) => Color::from([0.8, 0.6, 0.1, 1.0]),
        Layer::BooleanGroup(_) => Color::from([0.3, 0.3, 0.3, 1.0]),
        Layer::VectorNetwork(_) => Color::from([0.1, 0.5, 0.9, 1.0]),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_config_default() {
        let c = RasterConfig::default();
        assert!((c.dpi - 72.0).abs() < 0.1);
        assert!((c.scale - 1.0).abs() < 0.1);
    }

    #[test]
    fn raster_config_retina() {
        let c = RasterConfig::retina();
        assert!((c.scale - 2.0).abs() < 0.01);
        assert!((c.dpi - 144.0).abs() < 0.01);
    }

    #[test]
    fn raster_config_pixel_dims() {
        let page = ExportPage::new(100.0, 50.0);
        let c = RasterConfig { scale: 2.0, ..Default::default() };
        assert_eq!(c.pixel_width(&page), 200);
        assert_eq!(c.pixel_height(&page), 100);
    }

    #[test]
    fn raster_buffer_transparent() {
        let buf = RasterBuffer::transparent(4, 4);
        assert_eq!(buf.pixels.len(), 64);
        assert!(buf.pixels.iter().all(|&b| b == 0));
    }

    #[test]
    fn raster_buffer_with_color() {
        let buf = RasterBuffer::new(2, 2, Color::WHITE);
        assert_eq!(buf.pixels.len(), 16);
        assert_eq!(buf.pixels[0], 255);
        assert_eq!(buf.pixels[3], 255);
    }

    #[test]
    fn raster_buffer_set_get_pixel() {
        let mut buf = RasterBuffer::transparent(4, 4);
        buf.set_pixel(1, 2, Color::rgb(1.0, 0.0, 0.0));
        let p = buf.get_pixel(1, 2);
        assert_eq!(p.to_u8()[0], 255);
        assert_eq!(p.to_u8()[1], 0);
    }

    #[test]
    fn raster_buffer_out_of_bounds() {
        let mut buf = RasterBuffer::transparent(2, 2);
        buf.set_pixel(10, 10, Color::WHITE); // should not panic
        let p = buf.get_pixel(10, 10);
        assert!((p.a - 0.0).abs() < 0.01);
    }

    #[test]
    fn raster_buffer_fill_rect() {
        let mut buf = RasterBuffer::transparent(10, 10);
        buf.fill_rect(2.0, 2.0, 3.0, 3.0, Color::rgb(1.0, 0.0, 0.0));
        // Center pixel should be red
        let p = buf.get_pixel(3, 3);
        assert_eq!(p.to_u8()[0], 255);
        // Outside should be transparent
        let p2 = buf.get_pixel(0, 0);
        assert_eq!(p2.to_u8()[3], 0);
    }

    #[test]
    fn raster_buffer_fill_ellipse() {
        let mut buf = RasterBuffer::transparent(20, 20);
        buf.fill_ellipse(10.0, 10.0, 5.0, 5.0, Color::rgb(0.0, 1.0, 0.0));
        // Center should be filled
        let p = buf.get_pixel(10, 10);
        assert_eq!(p.to_u8()[1], 255);
        // Corner should be empty
        let p2 = buf.get_pixel(0, 0);
        assert_eq!(p2.to_u8()[3], 0);
    }

    #[test]
    fn raster_buffer_hline() {
        let mut buf = RasterBuffer::transparent(10, 5);
        buf.draw_hline(2, 8, 3, Color::WHITE);
        assert_eq!(buf.get_pixel(5, 3).to_u8()[0], 255);
        assert_eq!(buf.get_pixel(1, 3).to_u8()[3], 0);
    }

    #[test]
    fn raster_buffer_blend() {
        let mut buf = RasterBuffer::new(2, 2, Color::rgb(0.0, 0.0, 1.0));
        buf.blend_pixel(0, 0, Color::new(1.0, 0.0, 0.0, 0.5));
        let p = buf.get_pixel(0, 0);
        // Should be a mix of red and blue
        assert!(p.to_u8()[0] > 100);
        assert!(p.to_u8()[2] > 100);
    }

    #[test]
    fn encode_png_valid_signature() {
        let buf = RasterBuffer::transparent(1, 1);
        let mut out = Vec::new();
        encode_png(&buf, &mut out).unwrap();
        assert_eq!(&out[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn encode_png_empty_dimensions() {
        let buf = RasterBuffer { width: 0, height: 0, pixels: vec![] };
        let mut out = Vec::new();
        assert!(encode_png(&buf, &mut out).is_err());
    }

    #[test]
    fn encode_png_ihdr_present() {
        let buf = RasterBuffer::transparent(4, 4);
        let mut out = Vec::new();
        encode_png(&buf, &mut out).unwrap();
        // After 8-byte signature, length(4) + IHDR(4)
        assert_eq!(&out[12..16], b"IHDR");
    }

    #[test]
    fn encode_png_has_iend() {
        let buf = RasterBuffer::transparent(2, 2);
        let mut out = Vec::new();
        encode_png(&buf, &mut out).unwrap();
        // IEND chunk at the end: length(4) + "IEND" + CRC(4)
        let len = out.len();
        assert_eq!(&out[len - 8..len - 4], b"IEND");
    }

    #[test]
    fn crc32_known_value() {
        // CRC-32 of "IEND" is a known value
        let crc = crc32(b"IEND");
        assert_eq!(crc, 0xAE426082);
    }

    #[test]
    fn adler32_known_value() {
        let a = adler32(b"");
        assert_eq!(a, 1); // initial value
        let a2 = adler32(b"a");
        assert_eq!(a2, 0x00620062);
    }

    #[test]
    fn png_exporter_basic() {
        let page = ExportPage::new(10.0, 10.0);
        let exporter = PngExporter::new(page);
        let bytes = exporter.export_to_bytes(&[]).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(&bytes[0..8], &[137, 80, 78, 71, 13, 10, 26, 10]);
    }

    #[test]
    fn png_exporter_with_config() {
        let page = ExportPage::new(10.0, 10.0);
        let config = RasterConfig::retina();
        let exporter = PngExporter::new(page).with_config(config);
        assert!((exporter.config.scale - 2.0).abs() < 0.01);
    }

    #[test]
    fn png_exporter_to_writer() {
        let page = ExportPage::new(5.0, 5.0);
        let exporter = PngExporter::new(page);
        let mut out = Vec::new();
        exporter.export_to_writer(&[], &mut out).unwrap();
        assert!(!out.is_empty());
    }

    #[test]
    fn png_exporter_rasterize_empty() {
        let page = ExportPage::new(8.0, 8.0);
        let exporter = PngExporter::new(page);
        let buf = exporter.rasterize(&[]);
        assert_eq!(buf.width, 8);
        assert_eq!(buf.height, 8);
    }
}
