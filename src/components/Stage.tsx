import {
  ArrowLeft,
  Bell,
  Circle,
  LoaderCircle,
  Maximize,
  Minimize,
  Power,
  RotateCw,
  Smartphone,
  Square,
  Volume1,
  Volume2,
  VolumeX,
} from "lucide-react";
import { ClipboardEvent, KeyboardEvent, PointerEvent, ReactNode, useEffect, useLayoutEffect, useRef, useState, WheelEvent } from "react";
import { compatText } from "../lib/compat";
import {
  isTextInput,
  KEY_DOWN,
  KEY_UP,
  KEYCODE_APP_SWITCH,
  KEYCODE_HOME,
  KEYCODE_POWER,
  KEYCODE_VOLUME_DOWN,
  KEYCODE_VOLUME_UP,
  keycodeFor,
  metaState,
  MOTION_DOWN,
  MOTION_MOVE,
  MOTION_UP,
  toVideoPoint,
  wheelToScroll,
} from "../lib/input";
import type { Side } from "../lib/viewMode";
import { useStore } from "../store";
import { PhoneBar } from "./PhoneBar";

const BUTTON_PRIMARY = 1;
const SHELL_KEYS = new Set(["F10", "F11"]);

interface Props {
  fullscreen: boolean;
  phoneOnly: boolean;
  side: Side;
  onFullscreen(on: boolean): void;
  onExitPhoneOnly(): void;
  onDock(side: Side): void;
}

export function Stage({ fullscreen, phoneOnly, side, onFullscreen, onExitPhoneOnly, onDock }: Props) {
  const store = useStore();
  const { client, mirror, firstFrame, selected, t } = store;
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const areaRef = useRef<HTMLDivElement>(null);
  const [fit, setFit] = useState({ width: 0, height: 0 });
  const [muted, setMuted] = useState(false);
  const pressed = useRef(false);

  useEffect(() => {
    if (canvasRef.current) client.attach(canvasRef.current);
  }, [client]);

  const live = mirror.phase === "running";
  const showVideo = live && firstFrame;
  const canControl = selected?.platform === "android";
  const videoW = mirror.width || 9;
  const videoH = mirror.height || 19;

  // Scale the canvas to the largest size that fits, up or down, keeping the device's aspect ratio.
  useLayoutEffect(() => {
    const area = areaRef.current;
    if (!area) return;
    const update = () => {
      const scale = Math.min(area.clientWidth / videoW, area.clientHeight / videoH);
      setFit({ width: Math.floor(videoW * scale), height: Math.floor(videoH * scale) });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(area);
    return () => observer.disconnect();
  }, [videoW, videoH]);

  const point = (e: PointerEvent | WheelEvent) => {
    const canvas = canvasRef.current;
    if (!canvas) return null;
    const rect = canvas.getBoundingClientRect();
    const [w, h] = [client.videoWidth, client.videoHeight];
    const x = Math.max(rect.left, Math.min(e.clientX, rect.right));
    const y = Math.max(rect.top, Math.min(e.clientY, rect.bottom));
    const p = toVideoPoint(x, y, rect, w, h);
    return p && { ...p, w, h };
  };

  const touch = (action: number, e: PointerEvent, buttons: number) => {
    const p = point(e);
    if (p) client.send({ t: "touch", action, ...p, pressure: action === MOTION_UP ? 0 : 1, button: BUTTON_PRIMARY, buttons });
  };

  const onPointerDown = (e: PointerEvent<HTMLCanvasElement>) => {
    if (!canControl || !showVideo) return;
    e.currentTarget.focus();
    client.resumeAudio();
    if (e.button === 0) {
      e.currentTarget.setPointerCapture(e.pointerId);
      pressed.current = true;
      touch(MOTION_DOWN, e, BUTTON_PRIMARY);
    } else if (e.button === 2) {
      client.send({ t: "backOrScreenOn", action: KEY_DOWN });
      client.send({ t: "backOrScreenOn", action: KEY_UP });
    } else if (e.button === 1) {
      pressKey(KEYCODE_HOME);
    }
  };

  const onPointerMove = (e: PointerEvent) => {
    if (pressed.current) touch(MOTION_MOVE, e, BUTTON_PRIMARY);
  };

  const onPointerUp = (e: PointerEvent) => {
    if (pressed.current) {
      pressed.current = false;
      touch(MOTION_UP, e, 0);
    }
  };

  const onWheel = (e: WheelEvent) => {
    if (!canControl || !showVideo) return;
    const p = point(e);
    if (p) client.send({ t: "scroll", ...p, ...wheelToScroll(e.deltaX, e.deltaY, e.deltaMode), buttons: 0 });
  };

  const pressKey = (code: number) => {
    client.send({ t: "key", action: KEY_DOWN, code, repeat: 0, meta: 0 });
    client.send({ t: "key", action: KEY_UP, code, repeat: 0, meta: 0 });
  };

  const onKey = (e: KeyboardEvent, action: number) => {
    if (!canControl || SHELL_KEYS.has(e.code) || e.nativeEvent.isComposing) return;
    // Ctrl+V falls through so the browser raises the paste event below.
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyV") return;
    e.preventDefault();
    if (isTextInput(e)) {
      if (action === KEY_DOWN) client.send({ t: "text", text: e.key });
      return;
    }
    const code = keycodeFor(e.code);
    if (code !== null) client.send({ t: "key", action, code, repeat: e.repeat ? 1 : 0, meta: metaState(e) });
  };

  const onPaste = (e: ClipboardEvent) => {
    const text = e.clipboardData.getData("text/plain");
    if (canControl && text) {
      e.preventDefault();
      client.paste(text);
    }
  };

  const toggleMute = () => {
    client.setAudioEnabled(muted);
    setMuted(!muted);
  };

  return (
    <div className={`stage${fullscreen || phoneOnly ? " stage--fullscreen" : ""}${phoneOnly ? " stage--phone" : ""}`}>
      {phoneOnly && <PhoneBar side={side} onSide={onDock} onExit={onExitPhoneOnly} />}
      <div className="stage__glow" data-live={showVideo} />
      <div className="stage__area" ref={areaRef}>
        <div className={`bezel${showVideo ? " bezel--live" : ""}`} style={{ width: fit.width, height: fit.height }}>
          <canvas
            ref={canvasRef}
            className="bezel__screen"
            tabIndex={0}
            hidden={!showVideo}
            aria-label={selected?.name}
            onPointerDown={onPointerDown}
            onPointerMove={onPointerMove}
            onPointerUp={onPointerUp}
            onPointerCancel={onPointerUp}
            onWheel={onWheel}
            onKeyDown={(e) => onKey(e, KEY_DOWN)}
            onKeyUp={(e) => onKey(e, KEY_UP)}
            onPaste={onPaste}
            onContextMenu={(e) => e.preventDefault()}
          />
          {!showVideo && <Placeholder />}
        </div>
      </div>

      {showVideo && !phoneOnly && (
        <div className="rail" role="toolbar" aria-label={t("mirror.controls")}>
          {canControl ? (
            <>
              <RailButton label={t("mirror.back")} onClick={() => { client.send({ t: "backOrScreenOn", action: KEY_DOWN }); client.send({ t: "backOrScreenOn", action: KEY_UP }); }}>
                <ArrowLeft size={18} />
              </RailButton>
              <RailButton label={t("mirror.home")} onClick={() => pressKey(KEYCODE_HOME)}>
                <Circle size={17} />
              </RailButton>
              <RailButton label={t("mirror.recents")} onClick={() => pressKey(KEYCODE_APP_SWITCH)}>
                <Square size={15} />
              </RailButton>
              <hr />
              <RailButton label={t("mirror.volumeUp")} onClick={() => pressKey(KEYCODE_VOLUME_UP)}>
                <Volume2 size={18} />
              </RailButton>
              <RailButton label={t("mirror.volumeDown")} onClick={() => pressKey(KEYCODE_VOLUME_DOWN)}>
                <Volume1 size={18} />
              </RailButton>
              <RailButton label={t("mirror.power")} onClick={() => pressKey(KEYCODE_POWER)}>
                <Power size={18} />
              </RailButton>
              <RailButton label={t("mirror.rotate")} onClick={() => client.send({ t: "rotateDevice" })}>
                <RotateCw size={18} />
              </RailButton>
              <RailButton label={t("mirror.notifications")} onClick={() => client.send({ t: "expandNotifications" })}>
                <Bell size={18} />
              </RailButton>
              <hr />
              {mirror.audio === "active" && (
                <RailButton label={muted ? t("mirror.unmute") : t("mirror.mute")} onClick={toggleMute}>
                  {muted ? <VolumeX size={18} /> : <Volume2 size={18} />}
                </RailButton>
              )}
            </>
          ) : (
            <span className="rail__note">{t("mirror.viewOnly")}</span>
          )}
          <RailButton label={fullscreen ? t("mirror.exitFullscreen") : t("mirror.fullscreen")} onClick={() => onFullscreen(!fullscreen)}>
            {fullscreen ? <Minimize size={18} /> : <Maximize size={18} />}
          </RailButton>
        </div>
      )}
    </div>
  );
}

function RailButton({ label, onClick, children }: { label: string; onClick(): void; children: ReactNode }) {
  return (
    <button className="rail__button" onClick={onClick} title={label} aria-label={label}>
      {children}
    </button>
  );
}

function Placeholder() {
  const { mirror, selected, snapshot, t, settings } = useStore();
  const hasDevices = snapshot.devices.some((d) => d.state !== "disconnected");

  if (mirror.phase === "starting" || mirror.phase === "running") {
    const step = mirror.phase === "running" ? "mirror.step.waiting" : (`mirror.step.${mirror.step ?? "pushing"}` as const);
    return (
      <div className="placeholder">
        <LoaderCircle className="spin" size={30} />
        <p className="placeholder__step">{t(step)}</p>
      </div>
    );
  }

  if (!hasDevices) {
    return (
      <div className="placeholder">
        <Smartphone size={34} strokeWidth={1.4} />
        <h2>{t("mirror.empty.title")}</h2>
        <p>{t("mirror.empty.body")}</p>
        <ol>
          <li>{t("mirror.empty.step1")}</li>
          <li>{t("mirror.empty.step2")}</li>
          <li>{t("mirror.empty.step3")}</li>
        </ol>
      </div>
    );
  }

  if (selected && selected.capabilities.mirror) {
    return (
      <div className="placeholder">
        <Smartphone size={34} strokeWidth={1.4} />
        <h2>{t("mirror.ready.title")}</h2>
        <p>{t("mirror.ready.body")}</p>
      </div>
    );
  }

  const unsupported = selected?.state === "unsupported";
  return (
    <div className="placeholder">
      <Smartphone size={34} strokeWidth={1.4} />
      <h2>{unsupported ? t("mirror.unavailable.title") : t("mirror.attention.title")}</h2>
      <p>{selected?.compatibility ? compatText(selected.compatibility, settings?.language ?? "en") : t("mirror.empty.body")}</p>
    </div>
  );
}
