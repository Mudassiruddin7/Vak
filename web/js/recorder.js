// Microphone to a 16 kHz mono WAV, entirely in the browser.
// MediaRecorder gives whatever container the browser likes (webm, ogg, mp4); we decode it,
// resample it and write plain 16-bit PCM, which is what the speech API handles best.

const TARGET_RATE = 16000;
const SPEECH_FLOOR = 0.006;   // below this nothing in the clip is louder than a quiet room
const AudioCtx = window.AudioContext || window.webkitAudioContext;

export const micSupported = () => !!(navigator.mediaDevices?.getUserMedia && window.MediaRecorder && AudioCtx);

export async function startRecording({ maxSeconds = 29.5, onFrame, onTick, onAutoStop } = {}) {
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { channelCount: 1, echoCancellation: true, noiseSuppression: true, autoGainControl: true },
  });
  const ctx = new AudioCtx();
  const analyser = ctx.createAnalyser();
  analyser.fftSize = 128;
  analyser.smoothingTimeConstant = 0.55;
  ctx.createMediaStreamSource(stream).connect(analyser);

  const mime = ["audio/webm;codecs=opus", "audio/webm", "audio/ogg;codecs=opus", "audio/mp4"].find((m) => MediaRecorder.isTypeSupported(m));
  const recorder = new MediaRecorder(stream, mime ? { mimeType: mime } : undefined);
  const chunks = [];
  recorder.ondataavailable = (e) => { if (e.data && e.data.size) chunks.push(e.data); };

  const started = performance.now();
  const bins = new Uint8Array(analyser.frequencyBinCount);
  let raf = 0;
  let finished = false;

  const loop = () => {
    analyser.getByteFrequencyData(bins);
    onFrame?.(bins);
    const secs = (performance.now() - started) / 1000;
    onTick?.(secs);
    if (secs >= maxSeconds && !finished) { onAutoStop?.(); return; }
    raf = requestAnimationFrame(loop);
  };
  recorder.start(200);
  loop();

  const teardown = () => {
    finished = true;
    cancelAnimationFrame(raf);
    stream.getTracks().forEach((track) => track.stop());
    ctx.close().catch(() => {});
  };

  return {
    async stop() {
      if (recorder.state !== "inactive") {
        await new Promise((resolve) => { recorder.onstop = resolve; recorder.stop(); });
      }
      teardown();
      const blob = new Blob(chunks, { type: recorder.mimeType || mime || "audio/webm" });
      return toWav(blob);
    },
    cancel() {
      try { if (recorder.state !== "inactive") recorder.stop(); } catch { /* already stopped */ }
      teardown();
    },
  };
}

async function toWav(blob) {
  const decodeCtx = new AudioCtx();
  let decoded;
  try {
    decoded = await decodeCtx.decodeAudioData(await blob.arrayBuffer());
  } finally {
    decodeCtx.close().catch(() => {});
  }
  const offline = new OfflineAudioContext(1, Math.max(1, Math.ceil(decoded.duration * TARGET_RATE)), TARGET_RATE);
  const source = offline.createBufferSource();
  source.buffer = decoded;
  source.connect(offline.destination);
  source.start();
  const rendered = await offline.startRendering();
  const samples = rendered.getChannelData(0);

  if (loudestWindow(samples) < SPEECH_FLOOR) throw new Error("silent");
  return { blob: encodeWav(samples, TARGET_RATE), seconds: rendered.duration };
}

// RMS of the loudest 50 ms stretch. Averaging over the whole clip would punish long pauses,
// so a few quiet words in a long recording would wrongly count as silence.
function loudestWindow(samples, size = TARGET_RATE / 20) {
  let loudest = 0;
  for (let start = 0; start + size <= samples.length; start += size) {
    let sum = 0;
    for (let i = start; i < start + size; i++) sum += samples[i] * samples[i];
    loudest = Math.max(loudest, Math.sqrt(sum / size));
  }
  return loudest;
}

function encodeWav(samples, rate) {
  const buffer = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(buffer);
  const write = (offset, text) => { for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i)); };
  write(0, "RIFF");
  view.setUint32(4, 36 + samples.length * 2, true);
  write(8, "WAVE");
  write(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);          // PCM
  view.setUint16(22, 1, true);          // mono
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  write(36, "data");
  view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(44 + i * 2, s < 0 ? s * 0x8000 : s * 0x7fff, true);
  }
  return new Blob([view], { type: "audio/wav" });
}
