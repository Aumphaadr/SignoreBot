import Icon from "../Icon";
import { Hint, hintFireOnStart, hintInterval, hintNext, hintOffset, hintOverlay, hintOverlayAll, hintReaction, hintStatus } from "../Common/hints";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, errText, type PeriodicEvent, type TimerStatus } from "../../api";
import { defaultPeriodic, formatInterval } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import { reactionBadge } from "../Commands/CommandsTab";
import Modal, { ModalActions } from "../Common/Modal";
import ResponseEditor from "../Common/ResponseEditor";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import "./PeriodicTab.css";
import "./PeriodicTimeline.css";

const COLORS = ["#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#8b5cf6", "#ec4899", "#14b8a6", "#f97316", "#06b6d4", "#84cc16"];

// ------------------------------------------------------------------ таймлайн

function fmt(total: number) {
  const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60), s = total % 60;
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}
function fmtShort(total: number) {
  const h = Math.floor(total / 3600), m = Math.floor((total % 3600) / 60);
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}` : `${m} мин`;
}
const tickFor = (w: number) => (w <= 60 ? 5 : w <= 120 ? 10 : w <= 180 ? 15 : w <= 240 ? 20 : 30);
const snapFor = (w: number) => (w <= 60 ? 5 : w <= 120 ? 10 : w <= 180 ? 15 : 30);
function markers(interval: number, offset: number, windowSec: number) {
  const out: number[] = [];
  for (let t = ((offset % interval) + interval) % interval; t <= windowSec; t += interval) out.push(Math.round(t));
  return out;
}

function Timeline({ events, onOffset }: { events: PeriodicEvent[]; onOffset: (id: string, offset: number) => void }) {
  const [windowMin, setWindowMin] = useState(60);
  const windowSec = windowMin * 60;
  const snap = snapFor(windowMin) * 60;
  const [drag, setDrag] = useState<{ id: string; startX: number; startOffset: number; cur: number } | null>(null);
  const tracks = useRef<Record<string, HTMLDivElement | null>>({});

  const active = useMemo(() => events.filter((e) => e.enabled && e.intervalSec > 0).map((e, i) => ({ ...e, color: e.color || COLORS[i % COLORS.length], offset: drag?.id === e.id ? drag.cur : e.offsetSec })), [events, drag]);
  const ticks = useMemo(() => { const out: number[] = []; for (let t = 0; t <= windowSec; t += tickFor(windowMin) * 60) out.push(t); return out; }, [windowSec, windowMin]);
  const collisions = useMemo(() => {
    const map = new Map<number, number>();
    active.forEach((e) => markers(e.intervalSec, e.offset, windowSec).forEach((t) => map.set(t, (map.get(t) ?? 0) + 1)));
    return new Set([...map.entries()].filter(([, n]) => n > 1).map(([t]) => t));
  }, [active, windowSec]);

  useEffect(() => {
    if (!drag) return;
    const ev = active.find((e) => e.id === drag.id);
    const track = tracks.current[drag.id];
    if (!ev || !track) return;
    const move = (x: number) => {
      const w = track.getBoundingClientRect().width;
      let off = drag.startOffset + ((x - drag.startX) / w) * windowSec;
      off = Math.round(off / snap) * snap;
      off = ((off % ev.intervalSec) + ev.intervalSec) % ev.intervalSec;
      setDrag((d) => (d ? { ...d, cur: off } : d));
    };
    const onMove = (e: MouseEvent) => move(e.clientX);
    const onTouch = (e: TouchEvent) => { e.preventDefault(); move(e.touches[0].clientX); };
    const end = () => { onOffset(drag.id, Math.round(drag.cur)); setDrag(null); };
    window.addEventListener("mousemove", onMove); window.addEventListener("mouseup", end);
    window.addEventListener("touchmove", onTouch, { passive: false }); window.addEventListener("touchend", end);
    return () => { window.removeEventListener("mousemove", onMove); window.removeEventListener("mouseup", end); window.removeEventListener("touchmove", onTouch); window.removeEventListener("touchend", end); };
  }, [drag, active, windowSec, snap, onOffset]);

  if (active.length === 0) return null;
  const label = windowMin >= 60 ? `${Math.floor(windowMin / 60)} ч${windowMin % 60 ? ` ${windowMin % 60} мин` : ""}` : `${windowMin} мин`;
  return (
    <div className="periodic-timeline">
      <div className="timeline-header">
        <h3><Icon name="stopwatch" /> Таймлайн событий</h3>
        <div className="timeline-window-control">
          <label>Тайм-окно:</label>
          <input type="range" min={30} max={300} step={10} value={windowMin} onChange={(e) => setWindowMin(Number(e.target.value))} />
          <span className="window-value">{label}</span>
        </div>
        {collisions.size > 0 && <div className="collision-warning"><Icon name="warning" /> Наложения: {collisions.size}</div>}
      </div>
      <div className="timeline-body">
        <div className="timeline-axis" style={{ height: 28 }}>
          {ticks.map((t) => <div key={t} className="timeline-tick" style={{ left: `${(t / windowSec) * 100}%` }}><div className="tick-line" /><span className="tick-label">{fmtShort(t)}</span></div>)}
        </div>
        <div className="timeline-rows">
          {active.map((e) => (
            <div key={e.id} className="timeline-row" style={{ height: 40 }}>
              <div className="row-label" title={e.name}><span className="row-color-dot" style={{ backgroundColor: e.color }} /><span className="row-name">{e.name}</span></div>
              <div className="row-track" ref={(el) => { tracks.current[e.id] = el; }}>
                <div className="row-line" style={{ backgroundColor: e.color + "20" }} />
                {ticks.map((t) => <div key={t} className="row-grid-line" style={{ left: `${(t / windowSec) * 100}%` }} />)}
                {markers(e.intervalSec, e.offset, windowSec).map((t, i) => (
                  <div key={i} className={["timeline-marker", drag?.id === e.id ? "dragging" : "", collisions.has(t) ? "collision" : ""].filter(Boolean).join(" ")}
                    style={{ left: `${(t / windowSec) * 100}%`, backgroundColor: e.color, borderColor: collisions.has(t) ? "#fff" : e.color }}
                    title={`${e.name}: ${fmt(t)}${collisions.has(t) ? " (наложение)" : ""}`}
                    onMouseDown={(ev) => { ev.preventDefault(); setDrag({ id: e.id, startX: ev.clientX, startOffset: e.offset, cur: e.offset }); }}
                    onTouchStart={(ev) => setDrag({ id: e.id, startX: ev.touches[0].clientX, startOffset: e.offset, cur: e.offset })}
                  />
                ))}
              </div>
            </div>
          ))}
        </div>
      </div>
      <div className="timeline-hint"><Icon name="lightbulb" /> Перетаскивайте метки для настройки смещения — все метки одного события сдвигаются вместе.{drag && <span className="timeline-dragging-info"> Смещение: {fmt(drag.cur)}</span>}</div>
    </div>
  );
}

// ------------------------------------------------------------------ редактор

function PeriodicEditor({ initial, isNew, onSave }: { initial: PeriodicEvent; isNew: boolean; onSave: (e: PeriodicEvent) => void }) {
  const { config } = useAppState();
  const [e, setE] = useState(initial);
  const [interval, setInterval_] = useState(String(initial.intervalSec));
  const [offset, setOffset] = useState(String(initial.offsetSec));
  const iv = Math.max(10, parseInt(interval) || 10);
  const off = Math.max(0, parseInt(offset) || 0) % iv;
  const empty = !e.name.trim();
  const fmtOff = (s: number) => (s === 0 ? "0 сек" : formatInterval(s));
  return (
    <div className="periodic-editor">
      <ModalActions>
        <button onClick={() => onSave({ ...e, name: e.name.trim(), intervalSec: iv, offsetSec: off })} className="save-periodic-btn primary" disabled={empty}>{isNew ? <><Icon name="add"  /> Создать событие</> : <><Icon name="save"  /> Сохранить</>}</button>
      </ModalActions>
      <div className="periodic-editor-header">
        <div className="periodic-name-row">
          <div className="periodic-name-field"><label>Название события</label><input type="text" value={e.name} onChange={(ev) => setE({ ...e, name: ev.target.value })} placeholder="Название события" className="periodic-name-input" autoFocus={isNew} /></div>
          <div className="periodic-toggle-field">
            <label className="toggle-label"><span className="toggle-switch"><input type="checkbox" checked={e.enabled} onChange={() => setE({ ...e, enabled: !e.enabled })} /><span className="toggle-slider"></span></span><span className="toggle-text">{e.enabled ? "Включено" : "Выключено"}</span></label>
          </div>
        </div>
      </div>
      <div className="color-setting">
        <label><Icon name="palette" /> Цвет на таймлайне <Tooltip text="Если не выбран — назначится автоматически." /></label>
        <div className="color-presets">
          {COLORS.map((c) => <button key={c} className={`color-preset-btn ${e.color === c ? "active" : ""}`} style={{ backgroundColor: c }} onClick={() => setE({ ...e, color: e.color === c ? "" : c })} />)}
          <div className="color-custom"><input type="color" className="color-picker-input" value={e.color || "#9147ff"} onChange={(ev) => setE({ ...e, color: ev.target.value })} title="Свой цвет" /></div>
        </div>
      </div>
      <div className="interval-setting">
        <label><Icon name="stopwatch" /> Интервал (секунды) <Tooltip text="Как часто срабатывает событие. Минимум 10 секунд." /></label>
        <div className="interval-input-group">
          <input type="number" value={interval} onChange={(ev) => setInterval_(ev.target.value)} onBlur={() => setInterval_(String(iv))} min={10} className="interval-input" />
          <div className="interval-presets">
            {[[60, "1 мин"], [300, "5 мин"], [600, "10 мин"], [900, "15 мин"], [1800, "30 мин"], [3600, "1 час"]].map(([v, l]) => (
              <button key={v} onClick={() => setInterval_(String(v))} className={`preset-btn ${iv === v ? "active" : ""}`}>{l}</button>
            ))}
          </div>
        </div>
      </div>
      <div className="offset-setting">
        <label><Icon name="fast-forward" /> Смещение (секунды) <Tooltip text="Сдвиг сетки срабатываний относительно запуска бота. Разносит события с одинаковым интервалом. Можно двигать на таймлайне." /></label>
        <div className="offset-input-group">
          <input type="number" value={offset} onChange={(ev) => setOffset(ev.target.value)} onBlur={() => setOffset(String(off))} min={0} max={iv - 1} className="interval-input" />
          <span className="offset-preview">Первое срабатывание после запуска: <strong>{fmtOff(off === 0 ? iv : off)}</strong></span>
        </div>
        <label className="toggle-label" style={{ marginTop: 10 }}>
          <span className="toggle-switch"><input type="checkbox" checked={e.fireOnStart} onChange={(ev) => setE({ ...e, fireOnStart: ev.target.checked })} /><span className="toggle-slider"></span></span>
          <span>Сработать сразу при запуске бота</span>
          <Tooltip text="Один раз при старте, затем по сетке. Сохранение настроек не вызывает срабатывания." />
        </label>
      </div>
      <ResponseEditor value={e.response} onChange={(response) => setE({ ...e, response })} overlays={config.overlays} />
    </div>
  );
}

// ------------------------------------------------------------------ вкладка

export default function PeriodicTab() {
  const { config, setSection, status } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const events = config.periodicEvents;
  const [editing, setEditing] = useState<{ ev: PeriodicEvent; isNew: boolean } | null>(null);
  const [timers, setTimers] = useState<TimerStatus[]>([]);

  useEffect(() => {
    let alive = true;
    const tick = () => api.periodicStatus().then((t) => alive && setTimers(t)).catch(() => {});
    tick();
    const i = setInterval(tick, 5000);
    return () => { alive = false; clearInterval(i); };
  }, [events]);

  const save = (ev: PeriodicEvent) => {
    if (events.some((x) => x.id !== ev.id && x.name === ev.name)) return void showNotification("Событие с таким именем уже существует!", NOTIFICATION_TYPES.ERROR, 3000);
    const exists = events.some((x) => x.id === ev.id);
    setSection("periodicEvents", exists ? events.map((x) => (x.id === ev.id ? ev : x)) : [...events, ev]);
    setEditing(null);
    showNotification(`Событие «${ev.name}» ${exists ? "сохранено" : "создано"}`, NOTIFICATION_TYPES.SUCCESS, 2000);
  };
  const setOffset = useCallback((id: string, offsetSec: number) => setSection("periodicEvents", events.map((x) => (x.id === id ? { ...x, offsetSec } : x))), [events, setSection]);
  const trigger = async (ev: PeriodicEvent) => {
    try { await api.periodicTrigger(ev.id); showNotification(`Событие «${ev.name}» запущено вручную`, NOTIFICATION_TYPES.SUCCESS, 2000); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  };

  return (
    <div className="periodic-tab">
      <div className="periodic-header">
        <h2><Icon name="clock" /> Периодические события</h2>
        <p className="periodic-description">События, срабатывающие автоматически через равные промежутки времени.{!status?.running && " Таймеры работают, когда бот запущен (оба аккаунта авторизованы)."}</p>
        <div className="periodic-header-actions">
          <button onClick={() => setEditing({ ev: defaultPeriodic(), isNew: true })} className="create-periodic-btn"><Icon name="add"  /> Создать событие</button>
        </div>
      </div>
      <div className="periodic-list">
        {events.length === 0 && <div className="empty-periodic"><p><Icon name="inbox-empty" /> Периодических событий нет</p><p className="hint">Нажмите «Создать событие»</p></div>}
        {events.map((ev) => {
          const ov = ev.response.media.enabled && ev.response.media.overlay ? config.overlays.find((o) => o.id === ev.response.media.overlay) : null;
          const t = timers.find((x) => x.id === ev.id);
          return (
            <div key={ev.id} className={`periodic-card ${!ev.enabled ? "disabled" : ""}`}>
              <div className="periodic-card-header">
                <div className="periodic-title">
                  {ev.color && <span className="periodic-color-dot" style={{ backgroundColor: ev.color }} />}
                  <span className="periodic-name">{ev.name}</span>
                  <Hint text={hintStatus({ kind: "periodic", name: ev.name }, ev.enabled)}><span className={`periodic-status-badge ${ev.enabled ? "enabled" : "disabled"}`}>{ev.enabled ? "Вкл" : "Выкл"}</span></Hint>
                  <Hint text={hintInterval(ev.name, ev.intervalSec)}><span className="periodic-interval-badge"><Icon name="stopwatch" /> {formatInterval(ev.intervalSec)}</span></Hint>
                  {ev.offsetSec > 0 && <Hint text={hintOffset(ev.name, ev.offsetSec)}><span className="periodic-offset-badge"><Icon name="fast-forward" /> +{formatInterval(ev.offsetSec)}</span></Hint>}
                  {ev.fireOnStart && <Hint text={hintFireOnStart(ev.name)}><span className="periodic-type-badge"><Icon name="launch-rocket" /> при старте</span></Hint>}
                  <Hint text={hintReaction({ kind: "periodic", name: ev.name }, ev.response)}><span className="periodic-type-badge">{reactionBadge(ev.response)}</span></Hint>
                  {ov && <Hint text={hintOverlay(ov)}><span className="overlay-badge"><Icon name="overlay-screen" /> {ov.name}</span></Hint>}
                  {!ov && ev.response.media.enabled && <Hint text={hintOverlayAll(config.overlays)}><span className="overlay-badge all-overlays"><Icon name="broadcast" /> Все оверлеи</span></Hint>}
                  {t && ev.enabled && status?.running && <Hint text={hintNext(ev.name, t.nextInSec)}><span className="periodic-type-badge"><Icon name="hourglass" /> {formatInterval(t.nextInSec)}</span></Hint>}
                </div>
                <div className="periodic-actions">
                  <button onClick={() => void trigger(ev)} className="trigger-btn" title="Запустить сейчас"><Icon name="play"  /></button>
                  <button onClick={() => setSection("periodicEvents", events.map((x) => (x.id === ev.id ? { ...x, enabled: !x.enabled } : x)))} className={`status-toggle-btn ${ev.enabled ? "on" : "off"}`}><Icon name="power"  /></button>
                  <button onClick={() => setEditing({ ev, isNew: false })} className="edit-btn" title="Редактировать"><Icon name="edit"  /></button>
                  <button onClick={() => showConfirm(`Удалить событие «${ev.name}»?\n\nЭто действие нельзя отменить.`, () => { setSection("periodicEvents", events.filter((x) => x.id !== ev.id)); showNotification(`Событие «${ev.name}» удалено`, NOTIFICATION_TYPES.WARNING, 2000); })} className="delete-btn" title="Удалить"><Icon name="delete"  /></button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <Timeline events={events} onOffset={setOffset} />
      <Modal isOpen={!!editing} onClose={() => setEditing(null)} title={editing?.isNew ? "Создание периодического события" : `Редактирование события «${editing?.ev.name ?? ""}»`} size="xlarge">
        {editing && <PeriodicEditor key={editing.ev.id} initial={editing.ev} isNew={editing.isNew} onSave={save} />}
      </Modal>
    </div>
  );
}
