//! # Logos Prototyping Engine
//!
//! Provides interactive prototyping capabilities:
//! - **State machines** per container (artboard/frame/drawer)
//! - **Interaction triggers** (click, drag, hover, delay, swipe)
//! - **Smart Animate** with property interpolation and easing curves
//! - **Timeline** editor with keyframes
//! - **Preview mode** with live interaction handling
//! - **Flow viewer** for visualising navigation graphs

pub mod state_machine;
pub mod trigger;
pub mod animate;
pub mod timeline;
pub mod preview;
pub mod flow;
pub mod scroll;
pub mod overlay;

// Re-exports for convenience
pub use state_machine::{StateMachine, State, StateId, Transition, TransitionId};
pub use trigger::{Trigger, TriggerKind, Action, InteractionTarget};
pub use animate::{PropertyAnimation, EasingCurve, AnimationValue, Interpolatable};
pub use timeline::{Timeline, Keyframe, LoopMode, TimelineId};
pub use preview::{PreviewSession, PreviewEvent, PreviewState};
pub use flow::{FlowGraph, FlowNode, FlowEdge};
pub use scroll::{ScrollConfig, ScrollState, ScrollEvent, OverflowBehavior};
pub use overlay::{OverlayConfig, OverlayStack, ActiveOverlay, OverlayEvent, OverlayKind};
