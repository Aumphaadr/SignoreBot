import { useState, useEffect } from "react";
import Sidebar from "./components/Layout/Sidebar";
import StatusTab from "./components/Status/StatusTab";
import OverlayAlert from "./components/Status/OverlayAlert";
import UpdateBanner from "./components/Status/UpdateBanner";
import AuthTab from "./components/OAuth/AuthTab";
import OverlaysTab from "./components/Overlays/OverlaysTab";
import CommandsTab from "./components/Commands/CommandsTab";
import RewardsTab from "./components/Rewards/RewardsTab";
import EventsTab from "./components/Events/EventsTab";
import PeriodicTab from "./components/Periodic/PeriodicTab";
import ShoutoutsTab from "./components/Shoutouts/ShoutoutsTab";
import BanWordsTab from "./components/BanWords/BanWordsTab";
import MediaTab from "./components/Media/MediaTab";
import NotesTab from "./components/Notes/NotesTab";
import LogsTab from "./components/Logs/LogsTab";
import SettingsTab from "./components/Settings/SettingsTab";
import { NotificationProvider, setNotificationDefaults } from "./components/Notification";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AppStateProvider, useAppState } from "./state/AppState";
import "./components/Layout/MainContent.css";
import "./styles/App.css";

function Content({ tab, goTo }: { tab: string; goTo: (t: string) => void }) {
  switch (tab) {
    case "status": return <StatusTab goTo={goTo} />;
    case "auth": return <AuthTab />;
    case "overlays": return <OverlaysTab />;
    case "commands": return <CommandsTab />;
    case "rewards": return <RewardsTab />;
    case "events": return <EventsTab />;
    case "periodic": return <PeriodicTab />;
    case "shoutouts": return <ShoutoutsTab />;
    case "banwords": return <BanWordsTab />;
    case "media": return <MediaTab />;
    case "notes": return <NotesTab />;
    case "logs": return <LogsTab />;
    case "settings": return <SettingsTab />;
    default: return null;
  }
}

function Shell() {
  const [tab, setTab] = useState(() => { try { return localStorage.getItem("signorebot.tab") ?? "status"; } catch { return "status"; } });
  const goTo = (t: string) => { setTab(t); try { localStorage.setItem("signorebot.tab", t); } catch { /* ignore */ } };
  return (
    <div className="app-layout">
      <Sidebar active={tab} onChange={goTo} />
      <main className="main-content"><div className="main-content-container"><OverlayAlert goTo={goTo} /><UpdateBanner /><Content tab={tab} goTo={goTo} /></div></main>
    </div>
  );
}

export default function App() {
  return (
    <NotificationProvider>
      <AppStateProvider>
      <NotificationSettingsSync />
        <Shell />
      </AppStateProvider>
    </NotificationProvider>
  );
}

/** Передаёт «Настройки → Уведомления» провайдеру тостов, который живёт выше состояния. */
function NotificationSettingsSync() {
  const { config } = useAppState();
  useEffect(() => setNotificationDefaults(config.app.notificationSeconds, config.app.notificationsSticky), [config.app.notificationSeconds, config.app.notificationsSticky]);
  // масштаб панели — как Ctrl+плюс в браузере: текст, значки и отступы растут вместе
  useEffect(() => { getCurrentWebview().setZoom((config.app.uiZoom || 100) / 100).catch(() => {}); }, [config.app.uiZoom]);
  return null;
}
