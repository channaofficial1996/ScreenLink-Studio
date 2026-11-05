// Receives compressed media from the Rust bridge and renders it into the app's own canvas.
// H.264 is decoded by WebCodecs (GPU accelerated); nothing here is a separate player window.

import { codecFromConfig, concat } from "./h264";
import type { InputEvent, StreamStats } from "./types";

const KIND_VIDEO = 0;
const KIND_AUDIO = 1;
const KIND_JPEG = 2;
const FLAG_CONFIG = 1;
const FLAG_KEY = 2;
const HEADER_SIZE = 10;

const AUDIO_START_DELAY = 0.05;
const AUDIO_MAX_LATENCY = 0.25;
const MAX_DECODE_BACKLOG = 20;

export interface ClientCallbacks {
  onStats(stats: StreamStats): void;
  onFirstFrame(width: number, height: number): void;
  onVideoSize(width: number, height: number): void;
  onError(message: string): void;
}

export class MirrorClient {
  private canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private media: WebSocket | null = null;
  private input: WebSocket | null = null;
  private decoder: VideoDecoder | null = null;
  private pendingConfig: Uint8Array | null = null;
  private needsKeyFrame = true;
  private latestFrame: VideoFrame | null = null;
  private drawScheduled = false;
  private gotFirstFrame = false;
  private generation = 0;

  private audioDecoder: AudioDecoder | null = null;
  private audioContext: AudioContext | null = null;
  private audioGain: GainNode | null = null;
  private nextAudioTime = 0;
  private audioEnabled = true;

  private frames = 0;
  private bytes = 0;
  private statsTimer: number | null = null;
  private decoderKind: StreamStats["decoder"] = "unknown";
  private pasteSequence = 1;

  videoWidth = 0;
  videoHeight = 0;

  constructor(private callbacks: ClientCallbacks) {}

  attach(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d", { alpha: false, desynchronized: true });
  }

  connect(port: number, token: string) {
    this.disconnect();
    const generation = ++this.generation;
    const base = `ws://127.0.0.1:${port}`;

    const media = new WebSocket(`${base}/media?token=${token}`);
    media.binaryType = "arraybuffer";
    media.onmessage = (e) => {
      if (generation === this.generation) this.handleMessage(e.data as ArrayBuffer);
    };
    this.media = media;
    this.input = new WebSocket(`${base}/input?token=${token}`);

    this.statsTimer = window.setInterval(() => {
      this.callbacks.onStats({ fps: this.frames, kbps: Math.round((this.bytes * 8) / 1000), decoder: this.decoderKind });
      this.frames = 0;
      this.bytes = 0;
    }, 1000);
  }

  disconnect() {
    this.generation++;
    this.media?.close();
    this.input?.close();
    this.media = null;
    this.input = null;
    if (this.statsTimer !== null) window.clearInterval(this.statsTimer);
    this.statsTimer = null;
    this.closeVideo();
    this.closeAudio();
    this.gotFirstFrame = false;
    this.pendingConfig = null;
    this.needsKeyFrame = true;
    this.videoWidth = 0;
    this.videoHeight = 0;
  }

  /** Must run from a user gesture at least once, or the browser keeps audio suspended. */
  resumeAudio() {
    void this.audioContext?.resume();
  }

  setAudioEnabled(enabled: boolean) {
    this.audioEnabled = enabled;
    if (this.audioGain) this.audioGain.gain.value = enabled ? 1 : 0;
  }

  send(event: InputEvent) {
    if (this.input?.readyState === WebSocket.OPEN) this.input.send(JSON.stringify(event));
  }

  paste(text: string) {
    this.send({ t: "paste", text, sequence: this.pasteSequence++ });
  }

  /** Current preview as PNG bytes. Used for devices with no screenshot tool of their own. */
  async capturePng(): Promise<Uint8Array | null> {
    const canvas = this.canvas;
    if (!canvas || !this.gotFirstFrame) return null;
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
  }

  private handleMessage(buffer: ArrayBuffer) {
    const view = new DataView(buffer);
    const kind = view.getUint8(0);
    const flags = view.getUint8(1);
    const pts = Number(view.getBigUint64(2));
    const payload = new Uint8Array(buffer, HEADER_SIZE);
    this.bytes += buffer.byteLength;

    if (kind === KIND_VIDEO) this.handleVideo(flags, pts, payload);
    else if (kind === KIND_AUDIO) this.handleAudio(flags, pts, payload);
    else if (kind === KIND_JPEG) void this.handleJpeg(payload);
  }

  private handleVideo(flags: number, pts: number, payload: Uint8Array) {
    if (flags & FLAG_CONFIG) {
      this.configureVideo(payload);
      return;
    }
    const decoder = this.decoder;
    if (!decoder || decoder.state !== "configured") return;

    const key = (flags & FLAG_KEY) !== 0;
    if (this.needsKeyFrame && !key) return;

    // A stalled decoder cannot catch up frame by frame, so jump to a new key frame.
    if (decoder.decodeQueueSize > MAX_DECODE_BACKLOG) {
      this.needsKeyFrame = true;
      this.send({ t: "resetVideo" });
      return;
    }

    let data: Uint8Array<ArrayBuffer> = payload as Uint8Array<ArrayBuffer>;
    if (key && this.pendingConfig) {
      data = concat(this.pendingConfig, payload);
      this.pendingConfig = null;
    }
    this.needsKeyFrame = false;
    try {
      decoder.decode(new EncodedVideoChunk({ type: key ? "key" : "delta", timestamp: pts, data }));
    } catch {
      this.recoverVideo();
    }
  }

  private configureVideo(config: Uint8Array) {
    const codec = codecFromConfig(config);
    if (!codec) return;
    this.pendingConfig = config.slice();
    this.needsKeyFrame = true;

    if (!this.decoder || this.decoder.state === "closed") {
      this.decoder = new VideoDecoder({
        output: (frame) => this.handleFrame(frame),
        error: () => this.recoverVideo(),
      });
    }
    const options: VideoDecoderConfig = { codec, optimizeForLatency: true, hardwareAcceleration: "prefer-hardware" };
    void VideoDecoder.isConfigSupported(options).then((r) => {
      this.decoderKind = r.supported ? "hardware" : "software";
    });
    try {
      this.decoder.configure(options);
    } catch {
      this.decoderKind = "software";
      this.decoder.configure({ codec, optimizeForLatency: true });
    }
  }

  private recoverVideo() {
    this.closeVideo();
    this.needsKeyFrame = true;
    this.send({ t: "resetVideo" });
  }

  private handleFrame(frame: VideoFrame) {
    this.frames++;
    this.latestFrame?.close();
    this.latestFrame = frame;
    this.scheduleDraw();
  }

  private scheduleDraw() {
    if (this.drawScheduled) return;
    this.drawScheduled = true;
    requestAnimationFrame(() => {
      this.drawScheduled = false;
      const frame = this.latestFrame;
      if (!frame) return;
      this.latestFrame = null;
      this.drawImage(frame, frame.displayWidth, frame.displayHeight);
      frame.close();
    });
  }

  private drawImage(image: CanvasImageSource, width: number, height: number) {
    const { canvas, ctx } = this;
    if (!canvas || !ctx) return;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    if (width !== this.videoWidth || height !== this.videoHeight) {
      this.videoWidth = width;
      this.videoHeight = height;
      this.callbacks.onVideoSize(width, height);
    }
    ctx.drawImage(image, 0, 0);
    if (!this.gotFirstFrame) {
      this.gotFirstFrame = true;
      this.callbacks.onFirstFrame(width, height);
    }
  }

  private async handleJpeg(payload: Uint8Array) {
    const generation = this.generation;
    this.frames++;
    const bitmap = await createImageBitmap(new Blob([payload as Uint8Array<ArrayBuffer>], { type: "image/jpeg" }));
    if (generation === this.generation) this.drawImage(bitmap, bitmap.width, bitmap.height);
    bitmap.close();
  }

  private handleAudio(flags: number, pts: number, payload: Uint8Array) {
    if (typeof AudioDecoder === "undefined") return;
    if (flags & FLAG_CONFIG) {
      this.configureAudio(payload);
      return;
    }
    const decoder = this.audioDecoder;
    if (!decoder || decoder.state !== "configured") return;
    try {
      decoder.decode(new EncodedAudioChunk({ type: "key", timestamp: pts, data: payload as Uint8Array<ArrayBuffer> }));
    } catch {
      this.closeAudio();
    }
  }

  private configureAudio(opusHead: Uint8Array) {
    this.closeAudio();
    const context = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
    const gain = context.createGain();
    gain.gain.value = this.audioEnabled ? 1 : 0;
    gain.connect(context.destination);
    this.audioContext = context;
    this.audioGain = gain;
    this.nextAudioTime = 0;

    const decoder = new AudioDecoder({
      output: (data) => this.playAudio(data),
      error: () => this.closeAudio(),
    });
    decoder.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 2, description: opusHead.slice() });
    this.audioDecoder = decoder;
    void context.resume();
  }

  private playAudio(data: AudioData) {
    const context = this.audioContext;
    const gain = this.audioGain;
    if (!context || !gain || context.state !== "running") {
      data.close();
      return;
    }
    const buffer = context.createBuffer(data.numberOfChannels, data.numberOfFrames, data.sampleRate);
    for (let channel = 0; channel < data.numberOfChannels; channel++) {
      const samples = new Float32Array(data.numberOfFrames);
      data.copyTo(samples, { planeIndex: channel, format: "f32-planar" });
      buffer.copyToChannel(samples, channel);
    }
    data.close();

    const now = context.currentTime;
    if (this.nextAudioTime < now || this.nextAudioTime - now > AUDIO_MAX_LATENCY) {
      this.nextAudioTime = now + AUDIO_START_DELAY; // underrun or drift: resynchronize
    }
    const source = context.createBufferSource();
    source.buffer = buffer;
    source.connect(gain);
    source.start(this.nextAudioTime);
    this.nextAudioTime += buffer.duration;
  }

  private closeVideo() {
    if (this.decoder && this.decoder.state !== "closed") this.decoder.close();
    this.decoder = null;
    this.latestFrame?.close();
    this.latestFrame = null;
  }

  private closeAudio() {
    if (this.audioDecoder && this.audioDecoder.state !== "closed") this.audioDecoder.close();
    this.audioDecoder = null;
    void this.audioContext?.close();
    this.audioContext = null;
    this.audioGain = null;
  }
}
