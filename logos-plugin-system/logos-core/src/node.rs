use serde::{Deserialize, Serialize};
use uuid::Uuid;
// use std::collections::HashMap; // Removed unused import

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeType {
    Rectangle,
    Text,
    Frame,
    Component,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub node_type: NodeType,
    
    // Transform
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,

    // Style
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,

    // Text specific (optional for now, or use enum variant data)
    pub text_content: Option<String>,

    pub children: Vec<Uuid>,
    pub parent: Option<Uuid>,
}

impl Node {
    pub fn new(node_type: NodeType) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: format!("{:?}", node_type),
            node_type,
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            rotation: 0.0,
            fill: Some(Color { r: 217, g: 217, b: 217, a: 1.0 }), // #D9D9D9 (Figma default gray)
            stroke: None,
            stroke_width: 0.0,
            text_content: None,
            children: Vec::new(),
            parent: None,
        }
    }
}
