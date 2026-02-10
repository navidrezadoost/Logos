pub mod node;
pub mod document;
pub mod renderer;

pub use node::{Node, NodeType, Color};
pub use document::Document;
pub use renderer::Renderer;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
