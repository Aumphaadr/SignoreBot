import Icon from "../Icon";
import TestButton from "../Common/TestButton";
import { useState } from "react";
import { api, errText, type EventReaction } from "../../api";
import { EVENT_TYPES, defaultResponse } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import { reactionBadge } from "../Commands/CommandsTab";
import Modal, { ModalActions } from "../Common/Modal";
import { Hint, hintOverlay, hintOverlayAll, hintReaction, hintSkipGifted, hintStatus } from "../Common/hints";
import ResponseEditor from "../Common/ResponseEditor";
import { VariableBadge, VariableBadges } from "../Common/VariableBadge";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
import "./EventsTab.css";

const empty = (): EventReaction => ({ enabled: false, skipGifted: false, response: defaultResponse() });

export default function EventsTab() {
  const { config, setSection } = useAppState();
  const { showNotification } = useNotification();
  const [editing, setEditing] = useState<string | null>(null);
  const events = config.events;
  const get = (t: string) => events[t] ?? empty();
  const put = (t: string, e: EventReaction) => setSection("events", { ...events, [t]: e });

  const test = async (t: string) => {
    try { await api.eventTest(t); showNotification(`Тестовое событие «${EVENT_TYPES[t].label}» отправлено`, NOTIFICATION_TYPES.SUCCESS, 2000); }
    catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 3000); }
  };

  return (
    <div className="events-tab">
      <div className="events-header">
        <h2><Icon name="event-party" /> События Twitch</h2>
        <p className="events-description">Реакции бота на события канала. В текстах доступны переменные вроде <VariableBadges className="inline-variable-list" variables={["user", "tier", "viewers", "streakCount"]} />.</p>
      </div>
      <div className="events-list">
        {Object.entries(EVENT_TYPES).map(([t, meta]) => {
          const e = get(t);
          const ov = e.response.media.enabled && e.response.media.overlay ? config.overlays.find((o) => o.id === e.response.media.overlay) : null;
          const plain = meta.label.replace(/^[^\p{L}]+/u, ""); // имя без эмодзи — для подсказок
          return (
            <div key={t} className={`event-card ${!e.enabled ? "disabled" : ""}`}>
              <div className="event-card-header">
                <div className="event-title">
                  <span className="event-name"><Icon name={meta.icon} /> {meta.label}</span>
                  <Hint text={hintReaction({ kind: "event", name: plain }, e.response)}><span className="event-type-badge">{reactionBadge(e.response)}</span></Hint>
                  {t === "subscribe" && e.skipGifted && <Hint text={hintSkipGifted()}><span className="event-type-badge"><Icon name="gift" /> без подарочных</span></Hint>}
                  {ov && <Hint text={hintOverlay(ov)}><span className="overlay-badge"><Icon name="overlay-screen" /> {ov.name}</span></Hint>}
                  {!ov && e.response.media.enabled && <Hint text={hintOverlayAll(config.overlays)}><span className="overlay-badge all-overlays"><Icon name="broadcast" /> Все оверлеи</span></Hint>}
                </div>
                <div className="event-actions">
                  <button onClick={() => void test(t)} className="test-btn" title="Тест"><Icon name="play"  /> Тест</button>
                  <Hint text={hintStatus({ kind: "event", name: plain }, e.enabled)}><button onClick={() => { put(t, { ...e, enabled: !e.enabled }); showNotification(`Событие «${meta.label}» ${!e.enabled ? "включено" : "выключено"}`, NOTIFICATION_TYPES.INFO, 1500); }} className={`status-toggle-btn ${e.enabled ? "on" : "off"}`}><Icon name="power"  /></button></Hint>
                  <button onClick={() => setEditing(t)} className="edit-btn" title="Редактировать"><Icon name="edit"  /></button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <Modal isOpen={!!editing} onClose={() => setEditing(null)} title={`Редактирование события: ${editing ? EVENT_TYPES[editing].label : ""}`} size="xlarge">
        {editing && <EventEditor key={editing} type={editing} initial={get(editing)} onSave={(e) => { put(editing, e); setEditing(null); showNotification(`Событие «${EVENT_TYPES[editing].label}» сохранено`, NOTIFICATION_TYPES.SUCCESS, 2000); }} />}
      </Modal>
    </div>
  );
}

function EventEditor({ type, initial, onSave }: { type: string; initial: EventReaction; onSave: (e: EventReaction) => void }) {
  const { config } = useAppState();
  const [e, setE] = useState(initial);
  const meta = EVENT_TYPES[type];
  return (
    <div className="event-editor">
      <ModalActions><TestButton response={e.response} eventType={type} /><button onClick={() => onSave(e)} className="save-event-btn primary"><Icon name="save"  /> Сохранить</button></ModalActions>
      <div className="event-editor-header">
        <div className="event-meta">
          <h2><Icon name={meta.icon} /> {meta.label}</h2>
          <p className="event-description">{meta.description}</p>
          <div className="event-vars"><span className="vars-label">Доступные переменные:</span>{meta.vars.map((v) => <VariableBadge key={v} name={v} className="var-badge" />)}</div>
          {type === "subscribe" && (
            <label className="toggle-label" style={{ marginTop: 12 }}>
              <span className="toggle-switch"><input type="checkbox" checked={e.skipGifted} onChange={(ev) => setE({ ...e, skipGifted: ev.target.checked })} /><span className="toggle-slider"></span></span>
              <span>Не реагировать на подарочные подписки</span>
              <Tooltip text="Twitch присылает отдельное событие на каждую подаренную подписку и одно общее «Подарочная подписка». Включите, чтобы не было двойных алертов." />
            </label>
          )}
        </div>
      </div>
      <ResponseEditor value={e.response} onChange={(response) => setE({ ...e, response })} overlays={config.overlays} variables={meta.vars} />
    </div>
  );
}
