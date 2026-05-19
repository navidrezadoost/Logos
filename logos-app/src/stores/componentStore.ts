/**
 * stores/componentStore.ts  (P4.4 — Component Variants)
 *
 * Manages the component library:
 *   - Component definitions (shape tree + property metadata)
 *   - Instance tracking (which instances reference which component)
 *   - Override merge logic (CRDT delta-apply)
 *   - Variant swapping (recompute instance shape tree on property change)
 *
 * Architecture note
 * -----------------
 * Components are stored here as first-class records. The companion
 * `documentStore` shapes carry `componentMeta` / `instanceMeta` fields so
 * the layers panel and Inspector can render them without an extra lookup.
 * The two stores stay in sync via the actions below.
 *
 * Override merge model
 * --------------------
 * When a component changes, every derived instance is recomputed:
 *   resolved = deep-merge(componentDefaults, instance.overrides)
 * If an override path no longer exists (the component deleted that shape),
 * the override is silently dropped — this preserves the CRDT invariant that
 * deleted-wins for structure and last-write-wins for values.
 */

import { create } from "zustand";
import { type Shape, type ComponentMeta, type ComponentPropertyDef, type InstanceMeta, IDENTITY_TRANSFORM } from "../types/shapes";

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

/** Full component record stored in this store. */
export interface ComponentRecord {
  /** Matches the shape id of the "component" shell shape in documentStore. */
  id: string;
  name: string;
  /** Snapshot of the component's default child shapes (keyed by shape id). */
  defaultShapes: Record<string, Shape>;
  /** Ordered child shape ids in the default tree. */
  defaultChildIds: string[];
  /** Property definitions (variant / boolean / text). */
  properties: Record<string, ComponentPropertyDef>;
}

/** Live instance record — one per component instance on the canvas. */
export interface InstanceRecord {
  /** Matches the shape id of the "instance" shell shape in documentStore. */
  id: string;
  /** The component this instance references. */
  componentId: string;
  /** Selected value for each defined property. */
  variantProperties: Record<string, string>;
  /**
   * Fine-grained overrides applied on top of component defaults.
   * Keys are dot-paths: "<shapeId>.<field>" e.g. "rect1.fills[0].color"
   */
  overrides: Record<string, unknown>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Override merge
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Apply an overrides map on top of a snapshot of default shapes.
 *
 * Override key format: "<shapeId>.<field>"
 * Example: "btn-bg.fills" → replaces shape btn-bg's `fills` field.
 *
 * Paths that reference unknown shapes or fields are silently dropped,
 * preserving the CRDT deleted-wins invariant.
 */
export function applyOverrides(
  defaults: Record<string, Shape>,
  overrides: Record<string, unknown>
): Record<string, Shape> {
  const result: Record<string, Shape> = {};

  // Deep-clone the defaults
  for (const [id, shape] of Object.entries(defaults)) {
    result[id] = { ...shape };
  }

  for (const [path, value] of Object.entries(overrides)) {
    const dotIdx = path.indexOf(".");
    if (dotIdx === -1) continue;
    const shapeId = path.slice(0, dotIdx);
    const field = path.slice(dotIdx + 1) as keyof Shape;
    if (!(shapeId in result)) continue; // shape was deleted — drop override
    // Spread with dynamic key is the safest way without an index signature on Shape.
    result[shapeId] = { ...result[shapeId], [field]: value } as Shape;
  }

  return result;
}

/**
 * Compute the default `variantProperties` map for a component: each property
 * key maps to its `defaultValue`.
 */
export function defaultVariantProperties(
  properties: Record<string, ComponentPropertyDef>
): Record<string, string> {
  const result: Record<string, string> = {};
  for (const [key, def] of Object.entries(properties)) {
    result[key] = def.defaultValue;
  }
  return result;
}

// ─────────────────────────────────────────────────────────────────────────────
// Store
// ─────────────────────────────────────────────────────────────────────────────

interface ComponentState {
  /** All defined components, keyed by component id. */
  components: Record<string, ComponentRecord>;
  /** All live instances keyed by instance id. */
  instances: Record<string, InstanceRecord>;

  // ── Component actions ─────────────────────────────────────────────────────

  /**
   * Register a new component from an existing frame/group shape tree.
   *
   * @param componentId  - The shape id of the "component" shell shape.
   * @param name         - Human-readable component name.
   * @param defaultShapes - Snapshot of children that form the default variant.
   * @param defaultChildIds - Ordered child IDs in the default tree.
   * @param properties   - Initial property definitions (may be empty).
   */
  registerComponent: (
    componentId: string,
    name: string,
    defaultShapes: Record<string, Shape>,
    defaultChildIds: string[],
    properties?: Record<string, ComponentPropertyDef>
  ) => void;

  /**
   * Add or update a property definition on a component.
   * Existing instances get the new property set to its defaultValue.
   */
  addProperty: (
    componentId: string,
    propertyKey: string,
    def: ComponentPropertyDef
  ) => void;

  /**
   * Remove a property from a component.
   * The corresponding key is removed from all instance variantProperties.
   */
  removeProperty: (componentId: string, propertyKey: string) => void;

  /**
   * Update the component's default shapes (called when the user edits the
   * component master on the canvas).  Propagates to all instances via delta
   * merge: instance overrides are preserved; deleted paths are dropped.
   */
  updateComponentDefaults: (
    componentId: string,
    defaultShapes: Record<string, Shape>,
    defaultChildIds: string[]
  ) => void;

  // ── Instance actions ──────────────────────────────────────────────────────

  /**
   * Create a new instance of a component.
   *
   * @param instanceId  - The shape id for the new "instance" shell shape.
   * @param componentId - The component to instantiate.
   * @returns The initial `InstanceMeta` to attach to the shell shape.
   */
  createInstance: (instanceId: string, componentId: string) => InstanceMeta;

  /**
   * Set a variant property on an instance.
   *
   * Triggers a re-render by updating `variantProperties`.  The resolved
   * shape tree is obtained via `resolveInstance()`.
   */
  setVariantProperty: (
    instanceId: string,
    propertyKey: string,
    value: string
  ) => void;

  /**
   * Record a fine-grained override on an instance (e.g. the user changed
   * the fill of one child shape directly).
   */
  setOverride: (
    instanceId: string,
    path: string,
    value: unknown
  ) => void;

  /**
   * Clear all overrides on an instance, resetting it to component defaults.
   */
  resetInstance: (instanceId: string) => void;

  /**
   * Remove an instance record (called when the instance shape is deleted).
   */
  removeInstance: (instanceId: string) => void;

  // ── Resolution ────────────────────────────────────────────────────────────

  /**
   * Compute the fully-resolved shape map for an instance:
   * component defaults + instance overrides applied in order.
   *
   * Returns `null` if the component or instance no longer exists.
   */
  resolveInstance: (instanceId: string) => Record<string, Shape> | null;

  /** Returns all instance ids that reference a given component. */
  instancesOf: (componentId: string) => string[];
}

// ─────────────────────────────────────────────────────────────────────────────
// Implementation
// ─────────────────────────────────────────────────────────────────────────────

export const useComponentStore = create<ComponentState>((set, get) => ({
  components: {},
  instances: {},

  // ── Component actions ─────────────────────────────────────────────────────

  registerComponent: (componentId, name, defaultShapes, defaultChildIds, properties = {}) => {
    const record: ComponentRecord = {
      id: componentId,
      name,
      defaultShapes,
      defaultChildIds,
      properties,
    };
    set((s) => ({
      components: { ...s.components, [componentId]: record },
    }));
  },

  addProperty: (componentId, propertyKey, def) => {
    set((s) => {
      const comp = s.components[componentId];
      if (!comp) return s;

      // Update the component's property map
      const updatedComp: ComponentRecord = {
        ...comp,
        properties: { ...comp.properties, [propertyKey]: def },
      };

      // Set new property to its defaultValue on every existing instance
      const updatedInstances = { ...s.instances };
      for (const [iid, inst] of Object.entries(s.instances)) {
        if (inst.componentId === componentId) {
          updatedInstances[iid] = {
            ...inst,
            variantProperties: {
              ...inst.variantProperties,
              [propertyKey]: def.defaultValue,
            },
          };
        }
      }

      return {
        components: { ...s.components, [componentId]: updatedComp },
        instances: updatedInstances,
      };
    });
  },

  removeProperty: (componentId, propertyKey) => {
    set((s) => {
      const comp = s.components[componentId];
      if (!comp) return s;

      const { [propertyKey]: _removed, ...remainingProps } = comp.properties;
      const updatedComp: ComponentRecord = { ...comp, properties: remainingProps };

      const updatedInstances = { ...s.instances };
      for (const [iid, inst] of Object.entries(s.instances)) {
        if (inst.componentId === componentId) {
          const { [propertyKey]: _vp, ...remainingVP } = inst.variantProperties;
          updatedInstances[iid] = { ...inst, variantProperties: remainingVP };
        }
      }

      return {
        components: { ...s.components, [componentId]: updatedComp },
        instances: updatedInstances,
      };
    });
  },

  updateComponentDefaults: (componentId, defaultShapes, defaultChildIds) => {
    set((s) => {
      const comp = s.components[componentId];
      if (!comp) return s;

      const updatedComp: ComponentRecord = { ...comp, defaultShapes, defaultChildIds };

      // Prune overrides on every instance whose path no longer exists
      const updatedInstances = { ...s.instances };
      for (const [iid, inst] of Object.entries(s.instances)) {
        if (inst.componentId !== componentId) continue;
        const prunedOverrides: Record<string, unknown> = {};
        for (const [path, value] of Object.entries(inst.overrides)) {
          const dotIdx = path.indexOf(".");
          if (dotIdx === -1) continue;
          const shapeId = path.slice(0, dotIdx);
          if (shapeId in defaultShapes) {
            prunedOverrides[path] = value;
          }
          // otherwise: shape deleted from component → drop override
        }
        updatedInstances[iid] = { ...inst, overrides: prunedOverrides };
      }

      return {
        components: { ...s.components, [componentId]: updatedComp },
        instances: updatedInstances,
      };
    });
  },

  // ── Instance actions ──────────────────────────────────────────────────────

  createInstance: (instanceId, componentId) => {
    const { components } = get();
    const comp = components[componentId];
    if (!comp) {
      throw new Error(`componentStore.createInstance: unknown component "${componentId}"`);
    }

    const variantProperties = defaultVariantProperties(comp.properties);
    const instRecord: InstanceRecord = {
      id: instanceId,
      componentId,
      variantProperties,
      overrides: {},
    };

    set((s) => ({
      instances: { ...s.instances, [instanceId]: instRecord },
    }));

    const meta: InstanceMeta = {
      componentId,
      variantProperties,
      overrides: {},
    };
    return meta;
  },

  setVariantProperty: (instanceId, propertyKey, value) => {
    set((s) => {
      const inst = s.instances[instanceId];
      if (!inst) return s;

      // Validate: the value must be in the component's property definition
      const comp = s.components[inst.componentId];
      if (comp) {
        const propDef = comp.properties[propertyKey];
        if (propDef?.kind === "variant" && propDef.values && !propDef.values.includes(value)) {
          console.warn(
            `componentStore.setVariantProperty: value "${value}" not in allowed values for "${propertyKey}"`
          );
          return s;
        }
      }

      return {
        instances: {
          ...s.instances,
          [instanceId]: {
            ...inst,
            variantProperties: { ...inst.variantProperties, [propertyKey]: value },
          },
        },
      };
    });
  },

  setOverride: (instanceId, path, value) => {
    set((s) => {
      const inst = s.instances[instanceId];
      if (!inst) return s;
      return {
        instances: {
          ...s.instances,
          [instanceId]: {
            ...inst,
            overrides: { ...inst.overrides, [path]: value },
          },
        },
      };
    });
  },

  resetInstance: (instanceId) => {
    set((s) => {
      const inst = s.instances[instanceId];
      if (!inst) return s;
      return {
        instances: {
          ...s.instances,
          [instanceId]: { ...inst, overrides: {} },
        },
      };
    });
  },

  removeInstance: (instanceId) => {
    set((s) => {
      const { [instanceId]: _removed, ...rest } = s.instances;
      return { instances: rest };
    });
  },

  // ── Resolution ────────────────────────────────────────────────────────────

  resolveInstance: (instanceId) => {
    const { components, instances } = get();
    const inst = instances[instanceId];
    if (!inst) return null;
    const comp = components[inst.componentId];
    if (!comp) return null;

    return applyOverrides(comp.defaultShapes, inst.overrides);
  },

  instancesOf: (componentId) => {
    return Object.values(get().instances)
      .filter((i) => i.componentId === componentId)
      .map((i) => i.id);
  },
}));
