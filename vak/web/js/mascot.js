// Mitthu's brain: where the eyes look, when it blinks, what the speech bubble says.
import { MITTHU_SVG } from "./mascot-svg.js";
import { clamp, reducedMotion } from "./dom.js";

// where the eyes rest in each mood (-1..1 on both axes)
const GAZE = {
  idle: [0, 0], listening: [-0.55, 0.05], thinking: [-0.8, -0.95], speaking: [0, 0.1],
  worried: [0, 0.55], mixed: [0, 0.15], happy: [0, -0.1],
};
const FOLLOWS_POINTER = new Set(["idle", "happy", "mixed"]);

export function createMascot(host) {
  host.innerHTML = `<div class="mitthu-stage"><div class="bubble" hidden aria-live="polite"></div>${MITTHU_SVG}</div>`;
  const stage = host.firstElementChild;
  const svg = stage.querySelector(".mitthu");
  const bubble = stage.querySelector(".bubble");
  let mood = "idle";
  let lastPointer = 0;
  let blinkTimer = 0;

  const look = (x, y) => {
    svg.style.setProperty("--px", clamp(x, -1, 1).toFixed(3));
    svg.style.setProperty("--py", clamp(y, -1, 1).toFixed(3));
  };

  function onPointer(event) {
    if (!FOLLOWS_POINTER.has(mood) || reducedMotion()) return;
    lastPointer = performance.now();
    const box = svg.getBoundingClientRect();
    const cx = box.left + box.width / 2;
    const cy = box.top + box.height * 0.34;
    const dx = (event.clientX - cx) / Math.max(180, window.innerWidth * 0.3);
    const dy = (event.clientY - cy) / Math.max(180, window.innerHeight * 0.3);
    look(dx, dy);
  }
  window.addEventListener("pointermove", onPointer, { passive: true });

  function blink() {
    svg.classList.add("is-blinking");
    setTimeout(() => svg.classList.remove("is-blinking"), 110);
    const again = Math.random() < 0.22;
    blinkTimer = setTimeout(blink, again ? 260 : 2200 + Math.random() * 3600);
  }
  if (!reducedMotion()) blinkTimer = setTimeout(blink, 1600);

  function setMood(next) {
    mood = next;
    svg.dataset.mood = next === "mixed" ? "idle" : next;
    if (next !== "speaking") api.mouth(0);
    // pointer-following moods only go back to centre if the mouse has been still for a while
    if (!FOLLOWS_POINTER.has(next) || performance.now() - lastPointer > 4000) look(...(GAZE[next] || [0, 0]));
  }

  function say(text, { sticky = false } = {}) {
    if (!text) { bubble.hidden = true; return; }
    bubble.hidden = false;
    bubble.replaceChildren();
    const words = text.split(/(\s+)/);
    let i = 0;
    for (const w of words) {
      if (/^\s+$/.test(w)) { bubble.append(" "); continue; }
      const span = document.createElement("span");
      span.className = "w";
      span.textContent = w;
      span.style.animationDelay = `${i++ * 42}ms`;
      bubble.append(span);
    }
    // restart the pop-in so a new line feels like a new thought
    bubble.style.animation = "none";
    void bubble.offsetWidth;
    bubble.style.animation = "";
    bubble.dataset.sticky = sticky ? "1" : "";
  }

  const api = {
    el: stage,
    setMood,
    say,
    get mood() { return mood; },
    // 0..1 from the audio analyser; opens the jaw
    mouth(level) {
      const open = clamp(level, 0, 1);
      svg.style.setProperty("--open", open.toFixed(2));
      svg.classList.toggle("is-open", open > 0.12);
    },
    // without real audio levels the beak chatters on its own
    setAudioDriven(on) { svg.classList.toggle("no-audio", !on); },
    dock(newHost) { newHost.append(stage); },
    destroy() {
      window.removeEventListener("pointermove", onPointer);
      clearTimeout(blinkTimer);
    },
  };
  setMood("idle");
  return api;
}
