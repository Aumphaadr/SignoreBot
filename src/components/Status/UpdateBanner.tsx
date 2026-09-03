// Проверка обновлений при запуске (раз за сессию) и плашка на «Состоянии».

import Icon from "../Icon";
import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type UpdateInfo } from "../../api";
import { useAppState } from "../../state/AppState";

let checkedThisSession = false;

export default function UpdateBanner() {
  const { config } = useAppState();
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [hidden, setHidden] = useState(false);
  useEffect(() => {
    if (!config.updates.checkOnStart || checkedThisSession) return;
    checkedThisSession = true;
    api.updatesCheck().then(setInfo).catch(() => {});
  }, [config.updates.checkOnStart]);
  if (!info?.isNewer || hidden) return null;
  return (
    <div className="status-hint status-update-available mb-4">
      🆕 Доступна новая версия <strong>{info.latest}</strong> (у вас {info.current}).
      <div className="mt-2 flex gap-2">
        {info.url && <button className="primary small" onClick={() => void openUrl(info.url!)}><Icon name="download"  /> Открыть страницу релиза</button>}
        <button className="small" onClick={() => setHidden(true)}>Позже</button>
      </div>
    </div>
  );
}
