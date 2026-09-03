// Баннер «оверлеи не подключены» — над содержимым любой вкладки, пока ядро
// держит предупреждение в статусе (см. core.rs::overlay_watchdog).

import Icon from "../Icon";
import { useEffect, useState } from "react";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";

let lastToasted: string | null = null;

export default function OverlayAlert({ goTo }: { goTo: (tab: string) => void }) {
  const { status } = useAppState();
  const { showNotification } = useNotification();
  const alert = status?.overlayAlert ?? null;
  const [dismissed, setDismissed] = useState<string | null>(null);
  // Тост при появлении (или смене текста) предупреждения; один раз на текст
  // (StrictMode в dev вызывает эффект дважды).
  useEffect(() => {
    if (alert && alert !== lastToasted) { lastToasted = alert; showNotification(`${alert}`, NOTIFICATION_TYPES.WARNING, 8000); }
    if (!alert) lastToasted = null;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [alert]);
  if (!alert || dismissed === alert) return null;
  return (
    <div className="overlay-alert" role="alert">
      <Icon name="warning" className="overlay-alert-icon" />
      <div className="overlay-alert-text">{alert}</div>
      <div className="overlay-alert-actions">
        <button className="small" onClick={() => goTo("overlays")}>Оверлеи</button>
        <button className="small" onClick={() => setDismissed(alert)} title="Скрыть, пока ситуация не изменится">Скрыть</button>
      </div>
    </div>
  );
}
