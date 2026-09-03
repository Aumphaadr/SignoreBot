import Icon, { type IconName } from "../Icon";
import { useAppState } from "../../state/AppState";
import logo from "../../assets/logo.svg";
import "./Sidebar.css";

export const MENU: { id: string; label: string; icon: IconName }[] = [
  { id: "status", label: "Состояние", icon: "home" },
  { id: "auth", label: "Авторизация", icon: "key" },
  { id: "overlays", label: "Оверлеи", icon: "layers" },
  { id: "commands", label: "Команды", icon: "robot" },
  { id: "rewards", label: "Баллы канала", icon: "gift" },
  { id: "events", label: "События", icon: "calendar" },
  { id: "periodic", label: "Периодическое", icon: "clock" },
  { id: "shoutouts", label: "Шатауты", icon: "bullhorn" },
  { id: "banwords", label: "Банворды", icon: "ban" },
  { id: "media", label: "Медиа", icon: "media" },
  { id: "notes", label: "Заметки", icon: "note" },
  { id: "logs", label: "Логи", icon: "document" },
  { id: "settings", label: "Настройки", icon: "settings" },
];

export default function Sidebar({ active, onChange }: { active: string; onChange: (id: string) => void }) {
  const { status } = useAppState();
  const ok = status?.running && status.eventsub.connected;
  const warn = status && !status.running;
  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1><img src={logo} alt="" className="sidebar-logo" /> SignoreBot</h1>
        <p className="sidebar-subtitle"><span className={`sidebar-dot ${ok ? "ok" : warn ? "bad" : "warn"}`} /> {ok ? "в работе" : warn ? "остановлен" : "подключение…"}</p>
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
