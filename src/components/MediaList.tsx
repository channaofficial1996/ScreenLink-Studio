import { convertFileSrc } from "@tauri-apps/api/core";
import { FolderOpen, Play, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, errorMessage } from "../lib/api";
import { formatBytes } from "../lib/format";
import type { MediaItem } from "../lib/types";
import { useStore } from "../store";

export function useMedia(kind: MediaItem["kind"]) {
  const { mediaVersion, settings } = useStore();
  const [items, setItems] = useState<MediaItem[]>([]);
  useEffect(() => {
    let cancelled = false;
    void api.listMedia().then((all) => {
      if (!cancelled) setItems(all.filter((m) => m.kind === kind));
    });
    return () => {
      cancelled = true;
    };
  }, [kind, mediaVersion, settings?.saveDirectory]);
  return items;
}

function useActions() {
  const { toast, bumpMedia, t } = useStore();
  const run = (action: Promise<void>) => action.catch((e) => toast(errorMessage(e), "error"));
  return {
    open: (m: MediaItem) => run(api.openMedia(m.name)),
    reveal: (m: MediaItem) => run(api.revealMedia(m.name)),
    remove: (m: MediaItem) => {
      if (window.confirm(`${t("media.confirmDelete")}\n${m.name}`)) void run(api.deleteMedia(m.name)).then(bumpMedia);
    },
  };
}

export function RecordingList({ items }: { items: MediaItem[] }) {
  const { t } = useStore();
  const actions = useActions();
  return (
    <ul className="media-list">
      {items.map((m) => (
        <li key={m.name}>
          <button className="media-list__main" onClick={() => actions.open(m)} title={t("media.open")}>
            <Play size={15} />
            <span>{m.name}</span>
          </button>
          <span className="muted">{formatBytes(m.bytes)}</span>
          <button className="icon-button" onClick={() => actions.reveal(m)} title={t("media.reveal")} aria-label={t("media.reveal")}>
            <FolderOpen size={16} />
          </button>
          <button className="icon-button" onClick={() => actions.remove(m)} title={t("media.delete")} aria-label={t("media.delete")}>
            <Trash2 size={16} />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function ScreenshotGrid({ items }: { items: MediaItem[] }) {
  const { t } = useStore();
  const actions = useActions();
  return (
    <div className="shot-grid">
      {items.map((m) => (
        <figure key={m.name} className="shot">
          <button className="shot__image" onClick={() => actions.open(m)} title={t("media.open")}>
            <img src={convertFileSrc(m.path)} alt={m.name} loading="lazy" />
          </button>
          <figcaption>
            <span>{m.name.replace(/\.png$/, "")}</span>
            <button className="icon-button" onClick={() => actions.reveal(m)} title={t("media.reveal")} aria-label={t("media.reveal")}>
              <FolderOpen size={15} />
            </button>
            <button className="icon-button" onClick={() => actions.remove(m)} title={t("media.delete")} aria-label={t("media.delete")}>
              <Trash2 size={15} />
            </button>
          </figcaption>
        </figure>
      ))}
    </div>
  );
}
