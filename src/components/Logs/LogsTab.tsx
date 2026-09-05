import Icon from "../Icon";
import { copyText } from "../../api/clipboard";
import { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, errText, type LogEntry } from "../../api";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "./LogsTab.css";

const MAX = 2000;

function cls(e: LogEntry): string {
  if (e.level === "error") return "log-error";
  if (e.level === "warn") return "log-warning";
  const t = e.target;
  if (t.includes("banwords")) return "log-ban";
  if (t.includes("commands") || t.includes("chat")) return "log-command";
  if (t.includes("media") || t.includes("overlay") || t.includes("obs")) return "log-media";
  if (t.includes("auth") || t.includes("core") || t.includes("eventsub")) return "log-success";
  return "log-info";
}

export default function LogsTab() {
  const { showNotification } = useNotification();
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [filter, setFilter] = useState("");
  const [level, setLevel] = useState<"all" | "info" | "warn" | "error">("all");
  const [target, setTarget] = useState("all");
  const [paused, setPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const pausedRef = useRef(false);
  const endRef = useRef<HTMLDivElement>(null);
  pausedRef.current = paused;

  useEffect(() => {
    let un: (() => void) | undefined;
    void api.logHistory().then(setLogs);
    void api.onLog((e) => { if (!pausedRef.current) setLogs((p) => [...p, e].slice(-MAX)); }).then((u) => (un = u));
    return () => un?.();
  }, []);

  const targets = useMemo(() => [...new Set(logs.map((l) => l.target))].sort(), [logs]);
  const shown = useMemo(() => logs.filter((l) =>
    (level === "all" || (level === "info" ? l.level === "info" || l.level === "debug" : l.level === level)) &&
    (target === "all" || l.target === target) &&
    (!filter || l.message.toLowerCase().includes(filter.toLowerCase()))), [logs, level, target, filter]);

  useEffect(() => { if (autoScroll) endRef.current?.scrollIntoView({ behavior: "auto" }); }, [shown, autoScroll]);

  const copyLogs = async () => {
    if (!shown.length) return void showNotification("Нечего копировать", NOTIFICATION_TYPES.WARNING, 2000);
    try {
      await copyText(shown.map((l) => `[${new Date(l.ts).toLocaleTimeString("ru-RU")}] [${l.level}] [${l.target}] ${l.message}`).join("\n"));
      showNotification(`Скопировано записей: ${shown.length}`, NOTIFICATION_TYPES.SUCCESS, 2000);
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  };
  const exportLogs = async () => {
    if (!logs.length) return void showNotification("Нет логов для экспорта", NOTIFICATION_TYPES.WARNING, 2000);
    try {
      const path = await save({ defaultPath: `signorebot-logs-${new Date().toISOString().slice(0, 19).replace(/:/g, "-")}.txt`, filters: [{ name: "Текст", extensions: ["txt"] }] });
      if (!path) return;
      const n = await api.logExport(path);
      showNotification(`Логи экспортированы (${n} записей)`, NOTIFICATION_TYPES.SUCCESS, 2000);
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  };

  return (
    <div className="logs-tab">
      <div className="logs-header"><div className="logs-title"><h2><Icon name="clipboard" /> Логи</h2><div className="connection-status connected"><span className="connection-dot"></span><span>Живой поток</span></div></div></div>
      <div className="logs-controls">
        <div className="search-box">
          <Icon name="search" className="search-icon" />
          <input type="text" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Поиск по логам..." className="filter-input" />
          {filter && <button className="search-clear" onClick={() => setFilter("")}><Icon name="close" /> </button>}
        </div>
        <div className="filter-group" style={{ display: "flex", gap: 12, alignItems: "center" }}>
          <Icon name="filter" className="filter-icon" />
          <select value={level} onChange={(e) => setLevel(e.target.value as typeof level)} className="level-select">
            <option value="all">Все уровни</option><option value="info">Инфо</option><option value="warn">Предупреждения</option><option value="error">Ошибки</option>
          </select>
          <select value={target} onChange={(e) => setTarget(e.target.value)} className="level-select">
            <option value="all">Все модули</option>{targets.map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div className="control-buttons">
          <button onClick={() => setAutoScroll((a) => !a)} className={`control-btn ${autoScroll ? "active" : ""}`} title={autoScroll ? "Автоскролл вкл" : "Автоскролл выкл"}>{autoScroll ? <Icon name="play"  /> : <Icon name="pause"  />}</button>
          <button onClick={() => setPaused((p) => !p)} className={`control-btn ${paused ? "paused" : ""}`} title={paused ? "Возобновить приём" : "Пауза приёма"}>{paused ? <Icon name="play"  /> : <Icon name="pause"  />}</button>
          <button onClick={() => setLogs([])} className="control-btn" title="Очистить"><Icon name="delete"  /></button>
          <button onClick={() => void copyLogs()} className="control-btn" title="Скопировать показанные записи в буфер обмена"><Icon name="copy"  /></button>
          <button onClick={() => void exportLogs()} className="control-btn with-text" title="Сохранить показанные логи в файл"><Icon name="download"  /> Экспорт</button>
        </div>
      </div>
      <div className="logs-container">
        {shown.length === 0 ? (
          <div className="logs-empty">{logs.length === 0 ? <><p><Icon name="inbox-empty" /> Логов пока нет</p><p className="empty-hint">Логи появляются по мере работы бота</p></> : <><p><Icon name="search" /> Нет логов, соответствующих фильтру</p><button onClick={() => { setFilter(""); setLevel("all"); setTarget("all"); }} className="clear-filter-btn">Сбросить фильтры</button></>}</div>
        ) : (
          <div className="logs-list">
            {shown.map((l, i) => (
              <div key={`${l.ts}-${i}`} className={`log-entry ${cls(l)}`}>
                <span className="log-timestamp">{new Date(l.ts).toLocaleTimeString()}</span>
                <span className="log-timestamp" style={{ opacity: 0.6 }}>{l.target}</span>
                <span className="log-message">{l.message}</span>
              </div>
            ))}
            <div ref={endRef} />
          </div>
        )}
      </div>
      <div className="logs-footer"><span>Всего: {logs.length}</span><span>Показано: {shown.length}</span>{paused && <span className="paused-indicator"><Icon name="pause" /> Пауза</span>}</div>
    </div>
  );
}
