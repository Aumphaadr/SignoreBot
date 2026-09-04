// Дашборд: аккаунты, рантайм, сервер, оверлеи, миграция, быстрые действия.

import Icon from "../Icon";
import { copyText } from "../../api/clipboard";
import { useState, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, errText } from "../../api";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import UpdateBanner from "./UpdateBanner";
import "./StatusTab.css";

export default function StatusTab({ goTo }: { goTo: (tab: string) => void }) {
  const { status, reloadStatus, reloadConfig, config } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const [msg, setMsg] = useState("");
  if (!status) return null;
  const s = status;
  const acc = (st: typeof s.broadcaster, label: ReactNode) => (
    <div className={`status-tile ${st.state === "authorized" ? "ok" : st.state === "pending" ? "warn" : "bad"}`}>
      <div className="status-tile-label">{label}</div>
      <div className="status-tile-value">{st.state === "authorized" ? st.login : st.state === "pending" ? "ожидание кода…" : st.state === "invalid" ? "токен недействителен" : "не авторизован"}</div>
      {st.state === "authorized" && st.missingScopes.length > 0 && <div className="status-tile-sub"><Icon name="warning" /> не хватает прав</div>}
    </div>
  );
  const importLegacy = async () => {
    const p = await open({ multiple: false, filters: [{ name: "config.json", extensions: ["json"] }], title: "Выберите config.json старой версии" });
    if (!p || Array.isArray(p)) return;
    showConfirm("Импортировать настройки из этого файла?\n\nТекущие команды, награды, события и т.д. будут заменены (резервная копия сохранится). Медиа из папки public/media рядом с файлом будут скопированы.", async () => {
      try {
        const r = await api.configImportFile(p);
        await reloadConfig();
        showNotification(`Импортировано (v${r.report.fromVersion}), медиа: ${r.mediaImported}${r.mediaErrors.length ? `, ошибок: ${r.mediaErrors.length}` : ""}`, NOTIFICATION_TYPES.SUCCESS, 5000);
        r.report.notes.forEach((n) => showNotification(`${n}`, NOTIFICATION_TYPES.INFO, 8000));
      } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 6000); }
    });
  };
  const send = async () => {
    const t = msg.trim();
    if (!t) return;
    try { await api.chatSend(t); setMsg(""); } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 4000); }
  };

  return (
    <div className="status-tab">
      <div className="commands-header"><h2><Icon name="home" /> Состояние бота</h2><p className="commands-description">SignoreBot {s.version} · данные: <code>{s.dataDir}</code> <button className="small" onClick={() => void api.openDataDir()}><Icon name="folder-open"  /> открыть</button></p></div>

      <UpdateBanner />
      {s.migration && (
        <div className="status-migration">
          <strong><Icon name="package" /> Настройки перенесены из старого формата (v{s.migration.fromVersion}).</strong>
          <ul>{s.migration.notes.map((n, i) => <li key={i}>{n}</li>)}</ul>
          <button className="small" onClick={() => void api.migrationDismiss().then(reloadStatus)}>Понятно</button>
        </div>
      )}

      <div className="status-grid">
        {acc(s.broadcaster, <><Icon name="streamer-camera" /> Стример</>)}
        {acc(s.bot, <><Icon name="robot" /> Бот{config.accounts.sameAccount && " (тот же аккаунт)"}</>)}
        <div className={`status-tile ${s.running ? (s.eventsub.connected ? "ok" : "warn") : "bad"}`}>
          <div className="status-tile-label"><Icon name="settings" /> Ядро</div>
          <div className="status-tile-value">{s.running ? (s.eventsub.connected ? "работает" : "EventSub переподключается…") : "остановлено"}</div>
          <div className="status-tile-sub">{s.running ? `EventSub: ${s.eventsub.connected ? `${s.eventsub.subscriptions} подписок` : "нет связи"}` : "нужны оба аккаунта"}</div>
        </div>
        <div className={`status-tile ${s.server.running ? "ok" : "bad"}`}>
          <div className="status-tile-label"><Icon name="globe" /> Сервер оверлеев</div>
          <div className="status-tile-value">{s.server.running ? s.server.address : "не запущен"}</div>
          <div className="status-tile-sub">{s.server.error ?? (s.server.allowLan ? "доступен из локальной сети" : "только этот компьютер")}</div>
        </div>
      </div>

      {(s.broadcaster.state !== "authorized" || s.bot.state !== "authorized") && (
        <div className="status-hint"><Icon name="auth-lock" /> Чтобы бот заработал, авторизуйте оба аккаунта на вкладке <a href="#" onClick={(e) => { e.preventDefault(); goTo("auth"); }}>Авторизация</a>.</div>
      )}

      <h3 className="status-section-title"><Icon name="overlay-screen" /> Оверлеи</h3>
      {s.overlays.length === 0 ? (
        <div className="status-hint">Оверлеев нет. <a href="#" onClick={(e) => { e.preventDefault(); goTo("overlays"); }}>Создать</a>.</div>
      ) : (
        <div className="status-overlays">
          {s.overlays.map((o) => (
            <div key={o.id} className={`status-overlay ${o.connected ? "ok" : "bad"}`}>
              <span className="status-overlay-dot" />
              <span className="status-overlay-name">{o.name}</span>
              <span className="text-muted">/overlay/{o.path}</span>
              <span className="status-overlay-state">{o.connected ? `подключён${o.connections > 1 ? ` ×${o.connections}` : ""}` : "не подключён"}{o.pending > 0 ? ` · в очереди ${o.pending}` : ""}</span>
              <button className="small icon-only" title="Скопировать ссылку" onClick={() => void copyText(o.url).then(() => showNotification("URL скопирован", NOTIFICATION_TYPES.SUCCESS, 1500))}><Icon name="copy"  /></button>
              <button className="small icon-only" title="Открыть в браузере" onClick={() => void import("@tauri-apps/plugin-opener").then((m) => m.openUrl(o.url))}><Icon name="external-link"  /></button>
            </div>
          ))}
          <div className="flex gap-2 mt-2">
            <button className="small" onClick={() => void api.overlayClear(null, false)}>Очистить очереди</button>
            <button className="small danger" onClick={() => void api.overlayClear(null, true).then(() => showNotification("Все оверлеи остановлены", NOTIFICATION_TYPES.INFO, 1500))}><Icon name="stop"  /> Остановить всё</button>
          </div>
        </div>
      )}

      <h3 className="status-section-title"><Icon name="chat" /> Сообщение в чат от бота</h3>
      <div className="form-row">
        <input type="text" value={msg} onChange={(e) => setMsg(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void send()} placeholder={s.running ? "Текст сообщения…" : "Бот не запущен"} disabled={!s.running} />
        <button className="primary" onClick={() => void send()} disabled={!s.running || !msg.trim()} style={{ flex: "0 0 auto" }}><Icon name="send"  /> Отправить</button>
      </div>

      <h3 className="status-section-title"><Icon name="package" /> Перенос настроек</h3>
      <div className="status-hint">
        Команд: {config.commands.length}, наград: {config.rewards.length}, оверлеев: {config.overlays.length}. Если это новая установка — импортируйте <code>config.json</code> старой версии: настройки будут переведены в новый формат, медиа скопированы.
        <div className="mt-2"><button onClick={() => void importLegacy()}><Icon name="folder-open"  /> Импортировать config.json…</button></div>
      </div>
    </div>
  );
}
