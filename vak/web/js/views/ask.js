// Step one: say or type what to check. Voice goes through Saaras, text goes straight in.
import { $, html, mount, raw, sleep, reducedMotion } from "../dom.js";
import { icon } from "../icons.js";
import { t, getLang } from "../i18n.js";
import { api, ApiError } from "../api.js";
import { micSupported, startRecording } from "../recorder.js";
import { state } from "../state.js";

const MAX_SECONDS = 30;
const BARS = 30;

function sampleLabel(sample) {
  return sample.label[getLang()] || sample.label.en;
}

export function renderAsk(ctx) {
  const { panel, mascot, cfg } = ctx;
  const lang = state.uiLang === "en" ? "te" : state.uiLang;

  mount(panel, html`
    <form class="card ask view-in" id="ask-form" novalidate>
      ${cfg.has_key ? "" : html`<div class="callout bad" style="margin-bottom:16px">${t("err.nokey")}</div>`}
      <label for="ask-text" class="lbl">${t("ask.label")}</label>
      <div class="ask-field" id="ask-field">
        <textarea id="ask-text" rows="4" lang="${lang}" maxlength="1500" placeholder="${t("ask.placeholder")}">${state.askText}</textarea>
        <span class="count" id="ask-count"></span>
      </div>
      <div class="ask-actions">
        <button type="button" class="mic" id="mic" aria-label="${t("ask.mic")}" ${micSupported() ? "" : raw('disabled title="Microphone not available in this browser"')}>
          ${icon("mic")}
          <svg class="timer-ring" viewBox="0 0 82 82" aria-hidden="true"><circle cx="41" cy="41" r="36"/></svg>
        </button>
        <span class="mic-label" id="mic-label">${t("ask.mic")}</span>
        <span class="grow"></span>
        <button class="btn btn-primary" type="submit" id="ask-go">${t("ask.go")} ${icon("go", 'class="go"')}</button>
      </div>

      <div class="samples">
        <span class="eyebrow">${t("ask.samples")}</span>
        <div class="chips" id="sample-chips">
          ${cfg.samples.map((s) => html`<button type="button" class="chip" data-sample="${s.id}" lang="${s.lang}">${sampleLabel(s)}</button>`)}
        </div>
        <span class="eyebrow" style="margin-top:20px">${t("ask.seeSample")}</span>
        <div class="chips">
          <button type="button" class="chip" data-demo="steady"><span class="dot" style="background:var(--leaf-500)"></span>${t("grade.steady")}</button>
          <button type="button" class="chip" data-demo="shaky"><span class="dot" style="background:var(--mirchi)"></span>${t("grade.concerning")}</button>
        </div>
      </div>

      <details class="more" ${state.adv.system || state.adv.reference || state.adv.fresh ? "open" : ""}>
        <summary>${t("ask.more")}</summary>
        <div class="more-grid">
          <div class="wide"><label for="opt-system">${t("ask.system")}</label><textarea id="opt-system" rows="2" maxlength="1500">${state.adv.system}</textarea></div>
          <div class="wide"><label for="opt-ref">${t("ask.reference")}</label><textarea id="opt-ref" rows="2" maxlength="1500">${state.adv.reference}</textarea></div>
          <label class="switch wide"><input type="checkbox" id="opt-fresh" ${state.adv.fresh ? "checked" : ""}><span class="track"></span>${t("ask.fresh")}</label>
        </div>
      </details>
    </form>`);

  const form = $("#ask-form", panel);
  const field = $("#ask-field", panel);
  const box = $("#ask-text", panel);
  const count = $("#ask-count", panel);
  const mic = $("#mic", panel);
  const micLabel = $("#mic-label", panel);
  const go = $("#ask-go", panel);
  let recorder = null;
  let recLayer = null;
  let disposed = false;

  const limit = cfg.limits.prompt_chars;
  const updateCount = () => {
    count.textContent = `${box.value.length}`;
    count.classList.toggle("over", box.value.length > limit + 200);
    state.askText = box.value;
  };
  box.addEventListener("input", updateCount);
  updateCount();

  $("#opt-system", panel).addEventListener("input", (e) => { state.adv.system = e.target.value; });
  $("#opt-ref", panel).addEventListener("input", (e) => { state.adv.reference = e.target.value; });
  $("#opt-fresh", panel).addEventListener("change", (e) => { state.adv.fresh = e.target.checked; });

  // --- samples and bundled examples -------------------------------------------------------
  panel.addEventListener("click", (event) => {
    const chip = event.target.closest("[data-sample]");
    if (chip) {
      const sample = cfg.samples.find((s) => s.id === chip.dataset.sample);
      box.value = sample.text;
      state.overrides = sample.target ? { target: sample.target } : {};
      state.heard = "";
      updateCount();
      box.focus();
      mascot.setMood("happy");
      setTimeout(() => mascot.setMood("idle"), 900);
    }
    const demo = event.target.closest("[data-demo]");
    if (demo) ctx.openExample(demo.dataset.demo);
  });

  // --- voice -----------------------------------------------------------------------------------
  function showRecLayer() {
    recLayer = document.createElement("div");
    recLayer.className = "rec-layer";
    recLayer.innerHTML = `<div class="wave">${"<i></i>".repeat(BARS)}</div><span class="rec-hint"></span><span class="rec-time">0:00</span>`;
    $(".rec-hint", recLayer).textContent = t("ask.recording");
    field.append(recLayer);
    return [...recLayer.querySelectorAll(".wave i")];
  }

  async function beginRecording() {
    let bars;
    try {
      recorder = await startRecording({
        maxSeconds: MAX_SECONDS - 0.5,
        onFrame(bins) {
          bars ??= [];
          bins.length && bars.forEach((bar, i) => {
            const v = bins[Math.min(bins.length - 1, 1 + Math.floor((i / BARS) * (bins.length * 0.7)))] / 255;
            bar.style.transform = `scaleY(${Math.max(0.08, Math.min(1, v * 1.25)).toFixed(3)})`;
          });
        },
        onTick(secs) {
          const s = Math.floor(secs);
          const label = recLayer && $(".rec-time", recLayer);
          if (label) label.textContent = `0:${String(s).padStart(2, "0")}`;
        },
        onAutoStop: () => endRecording(),
      });
    } catch {
      ctx.toast(t("err.mic"), "bad");
      return;
    }
    bars = showRecLayer();
    mic.classList.add("is-recording");
    mic.setAttribute("aria-label", t("ask.micStop"));
    micLabel.textContent = t("ask.micStop");
    mascot.setMood("listening");
    mascot.say(t("bubble.listening"));
    ctx.pipeline("input", "active");
  }

  async function endRecording() {
    if (!recorder) return;
    const active = recorder;
    recorder = null;
    mic.classList.remove("is-recording");
    micLabel.textContent = t("ask.mic");
    recLayer?.remove();
    recLayer = null;
    field.classList.add("is-busy");
    field.insertAdjacentHTML("beforeend", '<div class="skeleton-lines"><i></i><i></i><i></i></div>');
    mascot.setMood("thinking");
    mascot.say(t("bubble.transcribing"));
    try {
      const { blob } = await active.stop();
      const heard = await api.transcribe(blob, "auto");
      if (disposed) return;
      field.classList.remove("is-busy");
      $(".skeleton-lines", field)?.remove();
      await typeInto(box, heard.text, () => disposed);
      state.heard = heard.text;
      state.overrides = {};
      updateCount();
      ctx.pipeline("input", "done");
      ctx.refreshBudget(true);
      mascot.setMood("happy");
      mascot.say(`${t("ask.heard")}: “${heard.text.length > 60 ? heard.text.slice(0, 58) + "…" : heard.text}”`);
      setTimeout(() => mascot.setMood("idle"), 1400);
    } catch (err) {
      console.error("voice input failed:", err);
      field.classList.remove("is-busy");
      $(".skeleton-lines", field)?.remove();
      ctx.pipeline("input", "idle");
      mascot.setMood("worried");
      mascot.say(t("bubble.oops"));
      setTimeout(() => mascot.setMood("idle"), 2200);
      ctx.toast(err instanceof ApiError ? err.message : err.message === "silent" ? t("err.mic") : t("err.generic"), "bad");
    }
  }

  mic.addEventListener("click", () => (recorder ? endRecording() : beginRecording()));

  // --- go ------------------------------------------------------------------------------------------
  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    const text = box.value.trim();
    if (!text) {
      box.focus();
      ctx.toast(t("ask.empty"));
      mascot.setMood("worried");
      setTimeout(() => mascot.setMood("idle"), 1200);
      return;
    }
    go.disabled = true;
    mascot.setMood("thinking");
    mascot.say(t("bubble.planning"));
    ctx.pipeline("input", "done");
    ctx.pipeline("orchestrator", "active");
    try {
      const overrides = { ...state.overrides };
      if (state.adv.reference.trim()) overrides.reference_output = state.adv.reference.trim();
      const out = await api.plan(text, overrides);
      if (state.adv.system.trim()) out.plan.target.system_prompt = state.adv.system.trim();
      state.plan = out.plan;
      state.estimates = out.estimates;
      state.opts.explainLang = state.uiLang === "hi" ? "hi" : "te";
      ctx.pipeline("orchestrator", "done");
      ctx.refreshBudget(true);
      ctx.go("plan");
    } catch (err) {
      go.disabled = false;
      ctx.pipeline("orchestrator", "idle");
      mascot.setMood("worried");
      mascot.say(t("bubble.oops"));
      setTimeout(() => mascot.setMood("idle"), 2400);
      ctx.toast(err.message || t("err.generic"), "bad");
    }
  });

  mascot.setMood("idle");
  mascot.say(t("bubble.idle"));
  return {
    dispose() {
      disposed = true;
      recorder?.cancel();
    },
  };
}

// Types the transcript into the box word by word, like the parrot is writing it down
async function typeInto(box, text, cancelled) {
  if (reducedMotion()) { box.value = text; return; }
  const words = text.split(/(\s+)/);
  const per = Math.min(70, 1400 / Math.max(1, words.length));
  box.value = "";
  for (const w of words) {
    if (cancelled()) return;
    box.value += w;
    if (w.trim()) await sleep(per);
  }
}
