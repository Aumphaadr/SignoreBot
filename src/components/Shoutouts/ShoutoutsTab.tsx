import Icon from "../Icon";
import { useCallback, useEffect, useState } from "react";
import { api, errText, type RaidShoutoutMode, type ShoutoutStatus } from "../../api";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import "./ShoutoutsTab.css";

const RAID_MODES: { value: RaidShoutoutMode; label: string; description: string }[] = [
  { value: "none", label: "Никто", description: "Рейды не добавляют пользователей в очередь shoutout." },
  { value: "listed", label: "Только из списка auto-shoutout", description: "Рейдер получит shoutout только если он есть в списке ниже." },
  { value: "unlisted", label: "Только кроме списка auto-shoutout", description: "Рейдер получит shoutout только если его нет в списке ниже." },
  { value: "all", label: "Все рейдеры", description: "Любой рейдер попадёт в очередь shoutout." },
];
const SOURCE: Record<string, string> = { message: "сообщение", raid: "рейд", manual: "ручной" };

export default function ShoutoutsTab() {
  const { config, setSection, onChanged } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const so = config.shoutout;
  const [name, setName] = useState("");
  const [st, setSt] = useState<ShoutoutStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => api.shoutoutStatus().then(setSt).catch(() => {}), []);
  useEffect(() => { void load(); const i = setInterval(() => void load(), 5000); return () => clearInterval(i); }, [load]);
  useEffect(() => onChanged("shoutout", () => void load()), [onChanged, load]);

  const add = () => {
    const n = name.trim().toLowerCase().replace(/^@/, "");
    if (!n) return void showNotification("Введите имя пользователя", NOTIFICATION_TYPES.WARNING, 2000);
    if (so.autoList.includes(n)) return void showNotification("Пользователь уже в списке", NOTIFICATION_TYPES.WARNING, 2000);
    setSection("shoutout", { ...so, autoList: [...so.autoList, n] });
    setName("");
    showNotification(`${n} добавлен в авто-шатаут`, NOTIFICATION_TYPES.SUCCESS, 2000);
  };
  const trigger = async (u: string) => {
    setBusy(true);
    try { await api.shoutoutTrigger(u); showNotification(`Шатаут для ${u} добавлен в очередь`, NOTIFICATION_TYPES.SUCCESS, 2000); void load(); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
    finally { setBusy(false); }
  };
  const cooldown = (ms: number) => {
    if (ms <= 0) return "Готов";
    const s = Math.ceil(ms / 1000), m = Math.floor(s / 60);
    return m > 0 ? `${m} мин ${s % 60} сек` : `${s} сек`;
  };
  const inQueue = (u: string) => st?.queue.some((q) => q.username.toLowerCase() === u) ?? false;
  const isDone = (u: string) => st?.done.some((d) => d.username === u) ?? false;
  const mode = RAID_MODES.find((m) => m.value === so.raidMode) ?? RAID_MODES[0];

  return (
    <div className="shoutouts-tab">
      <div className="shoutouts-header">
        <h2><Icon name="bullhorn" /> Автоматический шатаут</h2>
        <p className="shoutouts-description">Бот делает /shoutout пользователям из списка при их первом сообщении за сессию. Кулдаун между шатаутами — {Math.round(so.cooldownSec / 60)} мин.</p>
        <Tooltip text="Shoutout выполняется от имени стримера (нужны права moderator:manage:shoutouts). Twitch не даёт делать shoutout, когда стрим офлайн." />
      </div>
      <div className="shoutouts-content">
        <div className="shoutout-list-section">
          <h3><Icon name="launch-rocket" /> Шатаут для рейдов</h3>
          <div className="raid-mode-control">
            <label htmlFor="raid-shoutout-mode">Какие рейдеры должны получить шатаут?</label>
            <select id="raid-shoutout-mode" value={so.raidMode} onChange={(e) => { setSection("shoutout", { ...so, raidMode: e.target.value as RaidShoutoutMode }); showNotification(`Режим shoutout для рейдов: ${RAID_MODES.find((m) => m.value === e.target.value)?.label}`, NOTIFICATION_TYPES.SUCCESS, 2000); }} className="raid-mode-select">
              {RAID_MODES.map((m) => <option key={m.value} value={m.value}>{m.label}</option>)}
            </select>
          </div>
          <p className="raid-mode-description">{mode.description}</p>
        </div>

        <div className="shoutout-list-section">
          <h3><Icon name="users" /> Список для авто-шатаута</h3>
          <div className="shoutout-users-list">
            {so.autoList.length === 0 ? (
              <div className="empty-shoutout-list"><p><Icon name="inbox-empty" /> Список пуст</p><p className="hint">Добавьте пользователей для автоматического шатаута</p></div>
            ) : so.autoList.map((u) => (
              <div key={u} className="shoutout-user-card">
                <div className="shoutout-user-info">
                  <span className="shoutout-username">{u}</span>
                  {isDone(u) && <span className="shoutout-done-badge"><Icon name="success-badge" /> Выполнено</span>}
                  {inQueue(u) && <span className="shoutout-queue-badge"><Icon name="hourglass" /> В очереди</span>}
                </div>
                <div className="shoutout-user-actions">
                  <button className="shoutout-trigger-btn" onClick={() => void trigger(u)} disabled={busy} title="Шатаут вручную"><Icon name="bullhorn"  /> Шатаутнуть</button>
                  <button className="shoutout-remove-btn" onClick={() => showConfirm(`Удалить ${u} из списка авто-шатаутов?`, () => { setSection("shoutout", { ...so, autoList: so.autoList.filter((x) => x !== u) }); showNotification(`${u} удалён из авто-шатаута`, NOTIFICATION_TYPES.WARNING, 2000); })}><Icon name="delete"  /></button>
                </div>
              </div>
            ))}
          </div>
          <div className="add-shoutout-form">
            <input type="text" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} placeholder="Логин пользователя (например: twitchuser)" className="shoutout-input" />
            <button onClick={add} className="add-shoutout-btn"><Icon name="add"  /> Добавить</button>
          </div>
          <div className="add-shoutout-form" style={{ marginTop: 12 }}>
            <input type="text" placeholder="Шатаутнуть любого (логин)" className="shoutout-input" onKeyDown={(e) => { if (e.key === "Enter") { const v = (e.target as HTMLInputElement).value.trim(); if (v) { void trigger(v); (e.target as HTMLInputElement).value = ""; } } }} />
            <span className="form-hint" style={{ margin: 0, alignSelf: "center" }}>Enter — добавить в очередь без включения в список</span>
          </div>
        </div>

        {st && (
          <div className="shoutout-status-section">
            <div className="shoutout-status-header">
              <h3><Icon name="statistics" /> Статус шатаутов</h3>
              <button onClick={() => { void api.shoutoutReset().then(load); showNotification("Список выполненных шатаутов сброшен", NOTIFICATION_TYPES.SUCCESS, 2000); }} className="reset-shoutout-btn"><Icon name="refresh"  /> Сбросить выполненные</button>
            </div>
            <div className="shoutout-status-grid">
              <div className="status-card"><div className="status-icon"><Icon name="success-badge" /> </div><div className="status-info"><div className="status-label">Выполнено</div><div className="status-value">{st.done.length}</div></div></div>
              <div className="status-card"><div className="status-icon"><Icon name="hourglass" /> </div><div className="status-info"><div className="status-label">В очереди</div><div className="status-value">{st.queue.length}</div></div></div>
              <div className="status-card"><div className="status-icon"><Icon name="stopwatch" /> </div><div className="status-info"><div className="status-label">Кулдаун</div><div className="status-value">{cooldown(st.cooldownRemainingMs)}</div></div></div>
            </div>
            {st.queue.length > 0 && (
              <div className="queue-list"><strong>Очередь:</strong>
                <div className="queue-items">
                  {st.queue.map((q) => (
                    <span key={q.id} className="queue-item">
                      <span className="queue-item-text">{q.username}<span className="shoutout-source"> {SOURCE[q.source]}{st.currentId === q.id ? " · отправляется" : ""}</span></span>
                      <button type="button" className="queue-remove-btn" title="Удалить из очереди" disabled={st.currentId === q.id} onClick={() => api.shoutoutRemove(q.id).then(load).catch((e) => showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000))}><Icon name="close"  /></button>
                    </span>
                  ))}
                </div>
              </div>
            )}
            {st.done.length > 0 && (
              <div className="done-list"><strong>Авто-shoutout за сессию:</strong>
                <div className="done-items">{st.done.map((d) => <span key={d.username} className="done-item">{d.username}<span className="shoutout-source"> {d.sources.map((s) => SOURCE[s]).join(", ")}</span></span>)}</div>
              </div>
            )}
          </div>
        )}

        <div className="shoutout-info">
          <h4><Icon name="book-open" /> Как это работает</h4>
          <ul>
            <li>При первом сообщении пользователя из списка за сессию бот делает /shoutout</li>
            <li>Рейдеры обрабатываются по выбранному выше режиму</li>
            <li>Shoutout за сообщение не мешает более позднему shoutout за рейд, но рейд блокирует последующий авто-шатаут за сообщение</li>
            <li>Кулдаун между шатаутами — 2 минуты (ограничение Twitch)</li>
            <li>Twitch ограничивает повторный shoutout одному пользователю в течение часа; лишние записи можно удалить из очереди</li>
            <li>Список выполненных сбрасывается при перезапуске или кнопкой «Сбросить»</li>
          </ul>
        </div>
      </div>
    </div>
  );
}
