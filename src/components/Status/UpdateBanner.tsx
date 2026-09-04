// Плашка «доступна новая версия». Проверку делает ядро само (через 20 с после
// запуска и раз в 12 часов, если включено в настройках), результат приходит в
// статусе — поэтому плашка видна на любой вкладке, а не только на «Состоянии».

import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import Icon from "../Icon";
import { useAppState } from "../../state/AppState";

export default function UpdateBanner() {
  const { status } = useAppState();
  const [hidden, setHidden] = useState<string | null>(null);
  const info = status?.update ?? null;
  if (!info?.isNewer || hidden === info.latest) return null;
  return (
    <div className="status-hint status-update-available mb-4">
      <Icon name="new-item" /> Доступна новая версия <strong>{info.latest}</strong> (у вас {info.current}).
      <div className="mt-2 flex gap-2">
        {info.url && <button className="primary small" onClick={() => void openUrl(info.url!)}><Icon name="download" /> Открыть страницу релиза</button>}
        <button className="small" onClick={() => setHidden(info.latest ?? "")}>Позже</button>
      </div>
    </div>
  );
}
