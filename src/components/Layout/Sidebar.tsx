import Icon, { type IconName } from "../Icon";
import { useAppState } from "../../state/AppState";
import logo from "../../assets/logo.svg";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Hint } from "../Common/hints";
import "./Sidebar.css";

export const MENU: { id: string; label: string; icon: IconName }[] = [
  { id: "status", label: "Состояние", icon: "house" },
  { id: "auth", label: "Авторизация", icon: "lock-key" },
  { id: "overlays", label: "Оверлеи", icon: "monitor" },
  { id: "commands", label: "Команды", icon: "window-terminal" },
  { id: "rewards", label: "Баллы канала", icon: "channel-points" },
  { id: "events", label: "События", icon: "party-popper" },
  { id: "periodic", label: "Периодическое", icon: "clock" },
  { id: "shoutouts", label: "Шатауты", icon: "megaphone" },
  { id: "banwords", label: "Банворды", icon: "ban" },
  { id: "media", label: "Медиа", icon: "media" },
  { id: "notes", label: "Заметки", icon: "sticky-note" },
  { id: "logs", label: "Логи", icon: "file-text" },
  { id: "settings", label: "Настройки", icon: "gear-8" },
];

export default function Sidebar({ active, onChange }: { active: string; onChange: (id: string) => void }) {
  const { status } = useAppState();
  const ok = status?.running && status.eventsub.connected;
  const warn = status && !status.running;
  const upd = status?.update;
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1><img src={logo} alt="" className="sidebar-logo" /> SignoreBot</h1>
        <p className="sidebar-subtitle"><span className={`sidebar-dot ${ok ? "ok" : warn ? "bad" : "warn"}`} /> {ok ? "в работе" : warn ? "остановлен" : <><Icon name="loader" className="spinning" /> подключение…</>}</p>
        {upd?.isNewer && upd.url && (
          <Hint text={<>вышла версия <b>{upd.latest}</b>, у вас {upd.current}; кнопка открывает страницу скачивания на сайте</>}>
            <button className="sidebar-update-btn" onClick={() => void openUrl(upd.url!)}><Icon name="external-link" /> Обновить до {upd.latest}</button>
          </Hint>
        )}
      </div>
      <nav className="sidebar-nav">
        {MENU.map(({ id, label, icon }) => (
          <button key={id} className={`sidebar-btn ${active === id ? "active" : ""}`} onClick={() => onChange(id)}>
            <div className="sidebar-btn-icon"><Icon name={icon} /></div>
            <span className="sidebar-btn-label">{label}</span>
          </button>
        ))}
      </nav>
    </aside>
  );
}
