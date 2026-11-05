import type { TKey } from "../lib/i18n";
import type { DeviceState } from "../lib/types";
import { useStore } from "../store";

const tones: Record<DeviceState, string> = {
  connected: "ok",
  unauthorized: "warn",
  offline: "warn",
  disconnected: "idle",
  unsupported: "bad",
};

export function StatusBadge({ state }: { state: DeviceState }) {
  const { t } = useStore();
  return (
    <span className={`pill pill--${tones[state]}`}>
      <i className="pill__dot" />
      {t(`state.${state}` as TKey)}
    </span>
  );
}
