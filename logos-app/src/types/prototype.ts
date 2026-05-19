/**
 * types/prototype.ts
 *
 * Type definitions for P4.8 Prototyping.
 * Interactions are stored per source-shape in prototypeStore.
 * No changes to the Shape type are needed — all data lives in the store.
 */

export type PrototypeTrigger = "click" | "hover" | "delay";

export type PrototypeTransition =
  | "instant"
  | "dissolve"
  | "slide-left"
  | "slide-right"
  | "push-left"
  | "push-right";

export type PrototypeEasing = "ease" | "ease-in" | "ease-out" | "linear";

export interface PrototypeInteraction {
  /** Unique ID within the source shape's interaction list. */
  id: string;
  trigger: PrototypeTrigger;
  /** Delay in ms — only relevant when trigger === "delay". */
  delay?: number;
  /** ID of the target shape/frame to navigate to. */
  target: string;
  transition: PrototypeTransition;
  /** Transition duration in ms. Ignored for "instant". */
  duration: number;
  easing: PrototypeEasing;
}

/** Stored per source-shape ID in prototypeStore.interactions. */
export interface FramePrototype {
  interactions: PrototypeInteraction[];
}
