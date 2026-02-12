use serde::{Serialize, Deserialize};
use std::sync::{Arc, RwLock};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DocumentMetadata {
    pub author_id: Uuid,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SpatialHash {
    pub cell_size: f32,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct RenderContext {
    // Placeholder
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Document {
    pub id: Uuid,
    pub version: u32,
    pub root: Arc<RwLock<Page>>,
    pub metadata: DocumentMetadata,
}

impl Document {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            version: 1,
            root: Arc::new(RwLock::new(Page::new())),
            metadata: DocumentMetadata {
                author_id: Uuid::nil(),
                created_at: 0,
                updated_at: 0,
            }
        }
    }

    /// Adds a layer to the root page. Thread-safe.
    pub fn add_layer(&self, layer: Layer) -> Result<(), String> {
        let mut page = self.root.write().map_err(|e| e.to_string())?;
        page.layers.push(layer);
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Page {
    pub id: Uuid,
    pub name: String,
    pub layers: Vec<Layer>,
    pub spatial_index: Option<SpatialHash>,
}

impl Page {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            name: "Page 1".to_string(),
            layers: Vec::new(),
            spatial_index: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Layer {
    Rect(RectLayer),
    Ellipse(EllipseLayer),
    Text(TextLayer),
    Frame(FrameLayer),
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RectLayer { 
    pub id: Uuid, 
    pub bounds: Rect 
}

impl RectLayer {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            id: Uuid::new_v4(),
            bounds: Rect { x, y, width, height }
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EllipseLayer { 
    pub id: Uuid, 
    pub bounds: Rect 
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TextLayer { 
    pub id: Uuid, 
    pub content: String, 
    pub bounds: Rect 
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FrameLayer { 
    pub id: Uuid, 
    pub children: Vec<Layer>, 
    pub bounds: Rect 
}

pub mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_creation() {
        let doc = Document::new();
        assert_eq!(doc.version, 1);
        let root = doc.root.read().unwrap();
        assert_eq!(root.name, "Page 1");
    }

    #[test]
    fn test_layer_structure() {
        let rect = RectLayer::new(10.0, 20.0, 100.0, 50.0);
        let layer = Layer::Rect(rect);
        
        match layer {
            Layer::Rect(r) => {
                assert_eq!(r.bounds.x, 10.0);
                assert_eq!(r.bounds.width, 100.0);
            },
            _ => panic!("Wrong layer type"),
        }
    }
}
