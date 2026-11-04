// The one mutable bag of state the views read from, plus a tiny event bus.

export const STAGES = ["input", "orchestrator", "engine", "explain", "output"];

export const state = {
  uiLang: "en",
  view: "ask",
  cfg: null,
  askText: "",
  heard: "",
  overrides: {},          // e.g. a sample's practice-model target
  adv: { system: "", reference: "", fresh: false },
  plan: null,
  estimates: null,
  opts: { depth: "standard", types: ["consistency", "perturbation", "behavioral"], explainLang: "te", voice: false, voiceId: null },
  run: null,
  budget: null,
  sample: false,          // showing a bundled example rather than a real run
};

export function freshRun(id = "") {
  return {
    id, status: "running", phase: "", queued: false,
    progress: { done: 0, total: 0 },
    baseline: "", variants: [], tests: new Map(), answers: new Map(), units: [],
    report: null, explanations: {}, explainLang: "te", audio: {},
    stages: { input: "done", orchestrator: "done", engine: "idle", explain: "idle", output: "idle" },
    notices: [], error: "", plan: null,
  };
}

// Rebuild a run from a stored audit record (history, or a bundled example)
export function runFromRecord(record) {
  const run = freshRun(record.id);
  run.status = record.status;
  run.report = record.report;
  run.explanations = record.explanations || {};
  run.explainLang = record.language || "te";
  run.audio = Object.fromEntries(Object.entries(run.explanations).filter(([, v]) => v.audio).map(([k, v]) => [k, v.audio]));
  run.error = record.error || "";
  run.baseline = record.report?.baseline_output || "";
  run.units = record.report?.units || [];
  run.stages = { input: "done", orchestrator: "done", engine: "done", explain: "done", output: "done" };
  for (const test of record.report?.tests || []) run.tests.set(test.id, test);
  run.variants = (record.report?.tests || []).map((t) => ({ id: t.id, category: t.category, kind: t.kind, label: t.label, input: t.input }));
  run.progress = { done: run.variants.length, total: run.variants.length };
  return run;
}

export const bus = new EventTarget();
export const emit = (name, detail) => bus.dispatchEvent(new CustomEvent(name, { detail }));
export function on(name, fn) {
  const handler = (event) => fn(event.detail);
  bus.addEventListener(name, handler);
  return () => bus.removeEventListener(name, handler);
}
