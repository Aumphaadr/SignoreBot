import Icon from "../Icon";
import { Hint, hintOverlay, hintOverlayAll, hintReaction, hintRewardMissing, hintStatus } from "../Common/hints";
import { useCallback, useEffect, useState } from "react";
import { api, errText, type ChannelReward, type Reward } from "../../api";
import { defaultReward } from "../../api/defaults";
import { useAppState } from "../../state/AppState";
import { reactionBadge } from "../Commands/CommandsTab";
import Modal, { ModalActions } from "../Common/Modal";
import ResponseEditor from "../Common/ResponseEditor";
import { VariableBadge, VariableBadges } from "../Common/VariableBadge";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import "./RewardsTab.css";

const VARS = ["user", "message"];
const DESCR = { user: "Пользователь, активировавший награду.", message: "Текст, введённый при активации награды." };

function RewardSelector({ channel, existing, loading, onRefresh, onPick, onCancel }: { channel: ChannelReward[]; existing: Reward[]; loading: boolean; onRefresh: () => void; onPick: (r: ChannelReward) => void; onCancel: () => void }) {
  const [id, setId] = useState("");
  const available = channel.filter((r) => !existing.some((e) => e.rewardId === r.id));
  const picked = channel.find((r) => r.id === id);
  return (
    <div className="reward-selector">
      <div className="channel-rewards-info">
        <div className="info-header">
          <span><Icon name="clipboard" /> Награды канала</span>
          <button onClick={onRefresh} className="refresh-btn" disabled={loading}><Icon name="refresh" className={loading ? "spinning" : ""} /> {loading ? "Загрузка..." : "Обновить"}</button>
        </div>
        {!loading && channel.length === 0 && (
          <div className="no-rewards-warning">
            <p><Icon name="warning" /> Не удалось загрузить награды канала</p>
            <ul><li>Оба аккаунта должны быть авторизованы и бот запущен (вкладка «Состояние»)</li><li>На канале должны быть пользовательские награды</li></ul>
          </div>
        )}
        {!loading && channel.length > 0 && available.length === 0 && <div className="no-available-rewards"><p><Icon name="success-badge" /> Все доступные награды уже настроены</p></div>}
        {available.length > 0 && (
          <div className="form-group">
            <label>Выберите награду</label>
            <select value={id} onChange={(e) => setId(e.target.value)} className="reward-select">
              <option value="">-- Выберите награду --</option>
              {available.map((r) => <option key={r.id} value={r.id}>{r.title} ({r.cost}) {r.requiresInput ? "— с текстом" : ""}</option>)}
            </select>
          </div>
        )}
        {picked?.requiresInput && (
          <div className="selected-reward-info"><div className="reward-preview">
            <p className="reward-hint"><Icon name="edit" /> Награда требует ввод текста — переменная <VariableBadge name="message" description={DESCR.message} /> будет подставлена.</p>
          </div></div>
        )}
      </div>
      <div className="form-actions">
        <button onClick={onCancel} className="outline">Отмена</button>
        <button onClick={() => picked && onPick(picked)} className="primary" disabled={!picked}><Icon name="arrow-right"  /> Далее — настроить реакцию</button>
      </div>
    </div>
  );
}

export default function RewardsTab() {
  const { config, setSection, status, onChanged } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const rewards = config.rewards;
  const [channel, setChannel] = useState<ChannelReward[]>([]);
  const [loading, setLoading] = useState(false);
  const [editing, setEditing] = useState<{ reward: Reward | null; isNew: boolean } | null>(null);

  const running = !!status?.running;
  const load = useCallback(async () => {
    if (!running) { setChannel([]); return; }
    setLoading(true);
    try { setChannel(await api.rewardsChannel()); } catch (e) { setChannel([]); showNotification(`${errText(e)}`, NOTIFICATION_TYPES.WARNING, 3000); }
    finally { setLoading(false); }
  }, [showNotification, running]);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => onChanged("runtime", () => void load()), [onChanged, load]);

  const es = status?.eventsub;
  const save = (r: Reward) => {
    const exists = rewards.some((x) => x.id === r.id);
    setSection("rewards", exists ? rewards.map((x) => (x.id === r.id ? r : x)) : [...rewards, r]);
    setEditing(null);
    showNotification(`Реакция на «${r.rewardTitle}» ${exists ? "сохранена" : "создана"}`, NOTIFICATION_TYPES.SUCCESS, 2000);
  };

  return (
    <div className="rewards-tab">
      <div className="rewards-header">
        <h2><Icon name="gift" /> Награды за баллы канала</h2>
        <p className="rewards-description">Реакции бота на активацию наград. Переменные: <VariableBadges className="inline-variable-list" variables={VARS} descriptions={DESCR} /></p>
        <div className="eventsub-status-bar">
          <div className={`eventsub-indicator ${es?.connected ? "connected" : "disconnected"}`}>{es?.connected ? <><Icon name="status-connected" /> EventSub подключен</> : <><Icon name="status-disconnected" /> EventSub отключен</>}</div>
          {es?.connected && <span className="eventsub-subs-count"><Icon name="broadcast" /> Подписок: {es.subscriptions}</span>}
        </div>
      </div>
      <div className="rewards-list">
        {rewards.length === 0 && <div className="empty-rewards"><p><Icon name="inbox-empty" /> Реакции на награды не настроены</p><p className="hint">Нажмите «Добавить реакцию»</p></div>}
        {rewards.map((r) => {
          const info = channel.find((c) => c.id === r.rewardId);
          const ov = r.response.media.enabled && r.response.media.overlay ? config.overlays.find((o) => o.id === r.response.media.overlay) : null;
          return (
            <div key={r.id} className={`reward-card ${!r.enabled ? "disabled" : ""}`}>
              <div className="reward-card-header">
                <div className="reward-title">
                  {info?.image && <img src={info.image} alt="" className="reward-icon" />}
                  <span className="reward-name">{r.rewardTitle}</span>
                  {info && <span className="reward-cost">{info.cost} <Icon name="gem" /></span>}
                  {channel.length > 0 && !info && <Hint text={hintRewardMissing(r.rewardTitle)}><span className="reward-status-badge disabled">нет на канале</span></Hint>}
                  <Hint text={hintStatus({ kind: "reward", name: r.rewardTitle }, r.enabled)}><span className={`reward-status-badge ${r.enabled ? "enabled" : "disabled"}`}>{r.enabled ? "Вкл" : "Выкл"}</span></Hint>
                  <Hint text={hintReaction({ kind: "reward", name: r.rewardTitle }, r.response)}><span className="reward-type-badge">{reactionBadge(r.response)}</span></Hint>
                  {ov && <Hint text={hintOverlay(ov)}><span className="overlay-badge"><Icon name="overlay-screen" /> {ov.name}</span></Hint>}
                  {!ov && r.response.media.enabled && <Hint text={hintOverlayAll(config.overlays)}><span className="overlay-badge all-overlays"><Icon name="broadcast" /> Все оверлеи</span></Hint>}
                </div>
                <div className="reward-actions">
                  <button onClick={() => { setSection("rewards", rewards.map((x) => (x.id === r.id ? { ...x, enabled: !x.enabled } : x))); }} className={`status-toggle-btn ${r.enabled ? "on" : "off"}`}><Icon name="power"  /></button>
                  <button onClick={() => setEditing({ reward: r, isNew: false })} className="edit-btn" title="Редактировать"><Icon name="edit"  /></button>
                  <button onClick={() => showConfirm(`Удалить реакцию на награду "${r.rewardTitle}"?`, () => { setSection("rewards", rewards.filter((x) => x.id !== r.id)); showNotification("Реакция удалена", NOTIFICATION_TYPES.WARNING, 2000); })} className="delete-btn" title="Удалить"><Icon name="delete"  /></button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <div className="add-reward-section">
        <button className="add-reward-main-btn" onClick={() => setEditing({ reward: null, isNew: true })}><Icon name="add"  /> Добавить реакцию на награду</button>
      </div>
      <Modal isOpen={!!editing} onClose={() => setEditing(null)} size={editing?.reward ? "xlarge" : "medium"}
        title={!editing ? "" : !editing.reward ? "Выберите награду" : editing.isNew ? `Создание реакции на «${editing.reward.rewardTitle}»` : `Редактирование реакции на «${editing.reward.rewardTitle}»`}>
        {editing && !editing.reward && (
          <RewardSelector channel={channel} existing={rewards} loading={loading} onRefresh={() => void load()} onPick={(c) => setEditing({ reward: defaultReward(c.id, c.title), isNew: true })} onCancel={() => setEditing(null)} />
        )}
        {editing?.reward && <RewardEditor key={editing.reward.id} initial={editing.reward} isNew={editing.isNew} onSave={save} />}
      </Modal>
    </div>
  );
}

function RewardEditor({ initial, isNew, onSave }: { initial: Reward; isNew: boolean; onSave: (r: Reward) => void }) {
  const { config } = useAppState();
  const [r, setR] = useState(initial);
  return (
    <div className="reward-editor">
      <ModalActions>
        <button onClick={() => onSave(r)} className="save-reward-btn primary">{isNew ? <><Icon name="add"  /> Создать реакцию</> : <><Icon name="save"  /> Сохранить</>}</button>
      </ModalActions>
      <div className="reward-editor-header">
        <div className="reward-id-info"><span className="reward-id-label">Reward ID:</span><code className="reward-id-value">{r.rewardId}</code></div>
        <p className="reward-vars-hint"><Icon name="lightbulb" /> Доступные переменные: <VariableBadges className="inline-variable-list" variables={VARS} descriptions={DESCR} /></p>
      </div>
      <ResponseEditor value={r.response} onChange={(response) => setR({ ...r, response })} overlays={config.overlays} variables={VARS} />
    </div>
  );
}
