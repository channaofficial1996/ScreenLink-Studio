import { ArrowLeft, Bell, Circle, Maximize2, PanelLeft, PanelRight, Pin, PinOff, Square } from "lucide-react";
import { ReactNode, useState } from "react";
import { KEY_DOWN, KEY_UP, KEYCODE_APP_SWITCH, KEYCODE_HOME } from "../lib/input";
import { phoneMode, Side } from "../lib/viewMode";
import { useStore } from "../store";

/**
 * Hidden controls for Phone Only mode. The strip along the top edge is the only thing that reveals
 * them, so a screen recording shows nothing but the phone. It also drags the borderless window.
 */
export function PhoneBar({ side, onSide, onExit }: { side: Side; onSide(side: Side): void; onExit(): void }) {
  const { client, t, selected } = useStore();
  const [pinned, setPinned] = useState(true);
  const canControl = selected?.platform === "android";

  const pressKey = (code: number) => {
    client.send({ t: "key", action: KEY_DOWN, code, repeat: 0, meta: 0 });
    client.send({ t: "key", action: KEY_UP, code, repeat: 0, meta: 0 });
  };

  const togglePin = () => {
    setPinned(!pinned);
    void phoneMode.pin(!pinned);
  };

  return (
    <div className="phonebar-zone" data-tauri-drag-region>
      <div className="phonebar" data-tauri-drag-region>
        {canControl && (
          <>
            <BarButton label={t("mirror.back")} onClick={() => { client.send({ t: "backOrScreenOn", action: KEY_DOWN }); client.send({ t: "backOrScreenOn", action: KEY_UP }); }}>
              <ArrowLeft size={15} />
            </BarButton>
            <BarButton label={t("mirror.home")} onClick={() => pressKey(KEYCODE_HOME)}>
              <Circle size={14} />
            </BarButton>
            <BarButton label={t("mirror.recents")} onClick={() => pressKey(KEYCODE_APP_SWITCH)}>
              <Square size={13} />
            </BarButton>
            <BarButton label={t("mirror.notifications")} onClick={() => client.send({ t: "expandNotifications" })}>
              <Bell size={15} />
            </BarButton>
            <i className="phonebar__gap" />
          </>
        )}
        <BarButton label={t("view.dockLeft")} active={side === "left"} onClick={() => onSide("left")}>
          <PanelLeft size={15} />
        </BarButton>
        <BarButton label={t("view.dockRight")} active={side === "right"} onClick={() => onSide("right")}>
          <PanelRight size={15} />
        </BarButton>
        <BarButton label={t("view.pin")} active={pinned} onClick={togglePin}>
          {pinned ? <Pin size={15} /> : <PinOff size={15} />}
        </BarButton>
        <BarButton label={`${t("view.exitPhone")} (F10)`} onClick={onExit}>
          <Maximize2 size={15} />
        </BarButton>
      </div>
    </div>
  );
}

function BarButton({ label, active, onClick, children }: { label: string; active?: boolean; onClick(): void; children: ReactNode }) {
  return (
    <button className={`phonebar__button${active ? " phonebar__button--on" : ""}`} onClick={onClick} title={label} aria-label={label}>
      {children}
    </button>
  );
}
