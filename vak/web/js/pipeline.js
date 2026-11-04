// The five-stage strip from the architecture: Listen, Understand, Probe, Explain, Show & speak.
import { html, mount, $, $$ } from "./dom.js";
import { icon } from "./icons.js";
import { t } from "./i18n.js";
import { STAGES } from "./state.js";

const STAGE_ICON = { input: "mic", orchestrator: "compass", engine: "search", explain: "chat", output: "volume" };

export function renderPipeline(host, stages) {
  mount(host, html`
    <ol aria-label="Pipeline">
      ${STAGES.flatMap((name, i) => [
        html`<li class="node" data-stage="${name}">
          <span class="dot">${icon(STAGE_ICON[name])}</span>
          <span class="lbl">${t(`stage.${name}`)}</span>
          <span class="sub">${t(`stage.${name}.sub`)}</span>
        </li>`,
        i < STAGES.length - 1 ? html`<li class="link" data-after="${name}" aria-hidden="true"><i></i></li>` : "",
      ])}
    </ol>`);
  apply(host, stages);
}

export function apply(host, stages) {
  STAGES.forEach((name, i) => {
    const node = $(`.node[data-stage="${name}"]`, host);
    if (!node) return;
    const st = stages[name] || "idle";
    node.classList.toggle("is-active", st === "active");
    node.classList.toggle("is-done", st === "done");
    node.classList.toggle("is-error", st === "error");
    const link = $(`.link[data-after="${name}"]`, host);
    if (link) {
      const next = stages[STAGES[i + 1]] || "idle";
      link.classList.toggle("is-done", st === "done" && (next === "done" || next === "active"));
      link.classList.toggle("is-flowing", st === "done" && next === "active");
    }
  });
}

export function refreshLabels(host) {
  $$(".node", host).forEach((node) => {
    const name = node.dataset.stage;
    $(".lbl", node).textContent = t(`stage.${name}`);
    $(".sub", node).textContent = t(`stage.${name}.sub`);
  });
}
