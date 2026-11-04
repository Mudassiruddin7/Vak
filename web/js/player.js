// Plays the explanation, and reports how loud it is so the bars and the parrot's beak can follow along.

const AudioCtx = window.AudioContext || window.webkitAudioContext;

export class VoicePlayer {
  constructor({ onTime, onState, onFrame } = {}) {
    this.audio = new Audio();
    this.audio.preload = "auto";
    this.onTime = onTime;
    this.onState = onState;
    this.onFrame = onFrame;
    this.url = "";
    this.ctx = null;
    this.analyser = null;
    this.bins = null;
    this.raf = 0;

    this.audio.addEventListener("timeupdate", () => this.onTime?.(this.audio.currentTime, this.audio.duration || 0));
    this.audio.addEventListener("ended", () => { this.#stopLoop(); this.onState?.("ended"); });
    this.audio.addEventListener("pause", () => { this.#stopLoop(); if (!this.audio.ended) this.onState?.("paused"); });
    this.audio.addEventListener("play", () => { this.#startLoop(); this.onState?.("playing"); });
  }

  get duration() { return this.audio.duration || 0; }
  get playing() { return !this.audio.paused && !this.audio.ended; }

  async load(url) {
    if (this.url === url) return;
    this.url = url;
    this.audio.src = url;
    await new Promise((resolve, reject) => {
      this.audio.addEventListener("loadedmetadata", resolve, { once: true });
      this.audio.addEventListener("error", () => reject(new Error("audio failed to load")), { once: true });
    });
    this.onTime?.(0, this.audio.duration || 0);
  }

  #ensureGraph() {
    if (this.ctx || !AudioCtx) return;
    this.ctx = new AudioCtx();
    const source = this.ctx.createMediaElementSource(this.audio);
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 128;
    this.analyser.smoothingTimeConstant = 0.6;
    source.connect(this.analyser);
    this.analyser.connect(this.ctx.destination);
    this.bins = new Uint8Array(this.analyser.frequencyBinCount);
  }

  async play() {
    this.#ensureGraph();
    if (this.ctx?.state === "suspended") await this.ctx.resume();
    if (this.audio.ended) this.audio.currentTime = 0;
    await this.audio.play();
  }

  pause() { this.audio.pause(); }
  seek(ratio) { if (this.duration) this.audio.currentTime = ratio * this.duration; }
  setRate(rate) { this.audio.playbackRate = rate; }

  #startLoop() {
    cancelAnimationFrame(this.raf);
    const tick = () => {
      if (this.analyser) {
        this.analyser.getByteFrequencyData(this.bins);
        this.onFrame?.(this.bins);
      } else {
        this.onFrame?.(null);
      }
      this.raf = requestAnimationFrame(tick);
    };
    tick();
  }

  #stopLoop() {
    cancelAnimationFrame(this.raf);
    this.onFrame?.(new Uint8Array(0));
  }

  destroy() {
    this.audio.pause();
    this.#stopLoop();
    this.audio.removeAttribute("src");
    this.ctx?.close().catch(() => {});
  }
}

// 0..1 loudness from the low-to-mid band, where speech has most of its energy
export function loudness(bins) {
  if (!bins || !bins.length) return 0;
  const n = Math.min(bins.length, 28);
  let sum = 0;
  for (let i = 1; i < n; i++) sum += bins[i];
  return Math.min(1, (sum / (n - 1) / 255) * 2.2);
}

export const fmtTime = (secs) => {
  const s = Math.max(0, Math.round(secs || 0));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};
