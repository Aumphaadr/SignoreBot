// Тосты и диалог подтверждения (портировано из старой панели).

import Icon from "../Icon";
import { copyText } from "../../api/clipboard";
import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import "./Notification.css";

export const NOTIFICATION_TYPES = {
  SUCCESS: "success",
  ERROR: "error",
  WARNING: "warning",
  INFO: "info",
  CONFIRM: "confirm",
} as const;
export type NotificationType = (typeof NOTIFICATION_TYPES)[keyof typeof NOTIFICATION_TYPES];

interface Item {
  id: number;
  message: string;
  type: NotificationType;
  duration: number;
}
interface ConfirmState {
  message: string;
  onConfirm: () => void;
  onCancel?: () => void;
}
interface NotificationApi {
  showNotification: (message: string, type?: NotificationType, duration?: number) => number;
  showConfirm: (message: string, onConfirm: () => void, onCancel?: () => void) => void;
  closeNotification: (id: number) => void;
}

const NotificationContext = createContext<NotificationApi | null>(null);

export function useNotification(): NotificationApi {
  const ctx = useContext(NotificationContext);
  if (!ctx) throw new Error("useNotification вне NotificationProvider");
  return ctx;
}

function NotificationItem({ item, onClose }: { item: Item; onClose: () => void }) {
  const [progress, setProgress] = useState(100);
  useEffect(() => {
    const step = 50;
    const dec = 100 / (item.duration / step);
    const t = setInterval(() => setProgress((p) => Math.max(0, p - dec)), step);
    const c = setTimeout(onClose, item.duration);
    return () => {
      clearInterval(t);
      clearTimeout(c);
    };
  }, [item.duration, onClose]);
  const icon =
    item.type === "success" ? <Icon name="check"  /> : item.type === "error" ? <Icon name="close"  /> : item.type === "warning" ? <Icon name="warning"  /> : item.type === "confirm" ? <Icon name="help"  /> : <Icon name="info"  />;
  return (
    <div className={`notification ${item.type}`} onClick={onClose}>
      <div className="notification-content">
        <div className="notification-icon">{icon}</div>
        <span className="notification-message">{item.message}</span>
        <button className="notification-close" title="Копировать текст" onClick={(e) => { e.stopPropagation(); void copyText(item.message.replace(/^[^\p{L}\p{N}]+/u, "")); }}>
          <Icon name="copy"  />
        </button>
        <button className="notification-close" onClick={(e) => { e.stopPropagation(); onClose(); }}>
          <Icon name="close"  />
        </button>
      </div>
      <div className="notification-progress">
        <div className="progress-bar" style={{ width: `${progress}%` }} />
      </div>
    </div>
  );
}

export function ConfirmModal({ message, onConfirm, onCancel }: { message: string; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="confirm-overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onCancel(); }}>
      <div className="confirm-dialog">
        <div className="confirm-header">
          <Icon name="warning" className="confirm-icon warning" />
          <h3>Подтверждение</h3>
          <button className="confirm-close-btn" onClick={onCancel}><Icon name="close"  /></button>
        </div>
        <div className="confirm-body"><p>{message}</p></div>
        <div className="confirm-footer">
          <button className="confirm-btn cancel" onClick={onCancel}>Отмена</button>
          <button className="confirm-btn confirm" onClick={onConfirm} autoFocus>Подтвердить</button>
        </div>
      </div>
    </div>
  );
}

export function NotificationProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Item[]>([]);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);

  const closeNotification = useCallback((id: number) => setItems((p) => p.filter((n) => n.id !== id)), []);
  const showNotification = useCallback((message: string, type: NotificationType = "info", duration = 3000) => {
    const id = Date.now() + Math.random();
    setItems((p) => [...p.slice(-5), { id, message, type, duration }]);
    return id;
  }, []);
  const showConfirm = useCallback((message: string, onConfirm: () => void, onCancel?: () => void) => {
    setConfirm({ message, onConfirm, onCancel });
  }, []);

  return (
    <NotificationContext.Provider value={{ showNotification, showConfirm, closeNotification }}>
      {children}
      <div className="notifications-container">
        {items.map((it) => (
          <NotificationItem key={it.id} item={it} onClose={() => closeNotification(it.id)} />
        ))}
      </div>
      {confirm && (
        <ConfirmModal
          message={confirm.message}
          onConfirm={() => { confirm.onConfirm(); setConfirm(null); }}
          onCancel={() => { confirm.onCancel?.(); setConfirm(null); }}
        />
      )}
    </NotificationContext.Provider>
  );
}
