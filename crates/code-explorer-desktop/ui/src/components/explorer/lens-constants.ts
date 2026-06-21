import type { LensType } from "../../stores/app-store";

/** Maps lens type to visible edge rel_type strings. null = show all. */
export const LENS_EDGE_TYPES: Record<LensType, string[] | null> = {
  all: null,
  calls: ["CALLS"],
  structure: ["HAS_METHOD", "HAS_PROPERTY", "CONTAINED_IN", "DEFINED_IN"],
  heritage: ["EXTENDS", "IMPLEMENTS", "INHERITS"],
  impact: ["CALLS", "IMPORTS", "DEPENDS_ON"],
  "dead-code": null,
  tracing: null,
  hotspots: null,
  risk: null,
};
