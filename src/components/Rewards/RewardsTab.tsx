import Icon from "../Icon";
import TestButton from "../Common/TestButton";
import Tooltip from "../Tooltip";
import { Em, Hint, hintOverlay, hintOverlayAll, hintReaction, hintRewardMissing, hintStatus } from "../Common/hints";
import { useCallback, useEffect, useState } from "react";
import { api, errText, type ChannelReward, type ManagedCopyResult, type NewReward, type PendingRedemption, type Reward } from "../../api";
import RewardParamsForm, { defaultParams, paramsFromChannel, paramsProblem } from "./RewardParamsForm";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  const [redemptions, setRedemptions] = useState<PendingRedemption[]>([]);
  const [copyInfo, setCopyInfo] = useState<{ reward: Reward; result: ManagedCopyResult } | null>(null);
  const [creating, setCreating] = useState<NewReward | null>(null);
  const [creatingBusy, setCreatingBusy] = useState(false);
  const createOnTwitch = async () => {
    if (!creating) return;
    const problem = paramsProblem(creating);
    if (problem) { showNotification(problem, NOTIFICATION_TYPES.WARNING, 3000); return; }
    setCreatingBusy(true);
    try {
      const created = await api.rewardCreateTwitch(creating);
      setCreating(null);
      showNotification(`Награда «${created.title}» создана на Twitch. Картинку ей можно загрузить в панели Twitch — приложениям Twitch этого не даёт.`, NOTIFICATION_TYPES.SUCCESS, 8000);
      void load();
      setEditing({ reward: { ...defaultReward(created.id, created.title), managed: true }, isNew: true });
    } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 8000); }
    finally { setCreatingBusy(false); }
  };
  const loadRedemptions = useCallback(() => { api.redemptionsList().then(setRedemptions).catch(() => {}); }, []);
  useEffect(() => { loadRedemptions(); }, [loadRedemptions]);

  const running = !!status?.running;
  const load = useCallback(async () => {
    if (!running) { setChannel([]); return; }
    setLoading(true);
    try { setChannel(await api.rewardsChannel()); } catch (e) { setChannel([]); showNotification(`${errText(e)}`, NOTIFICATION_TYPES.WARNING, 3000); }
    finally { setLoading(false); }
  }, [showNotification, running]);
  useEffect(() => { void load(); }, [load]);
  useEffect(() => onChanged("runtime", () => void load()), [onChanged, load]);
  useEffect(() => onChanged("rewards", () => { void load(); loadRedemptions(); }), [onChanged, load, loadRedemptions]);
  const openQueue = async () => { try { await openUrl(await api.rewardsQueueUrl()); } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 4000); } };
  const makeCopy = (r: Reward) => showConfirm(
    `Создать через бота копию награды «${r.rewardTitle}»?\n\nКопия получит те же цену, подсказку и лимиты и название «${r.rewardTitle} (бот)». Реакция SignoreBot перейдёт на копию. Оригинал останется на канале — его нужно будет удалить в панели Twitch, после чего пометку «(бот)» можно убрать одной кнопкой.`,
    async () => {
      try { const result = await api.rewardCreateManagedCopy(r.id); setCopyInfo({ reward: r, result }); void load(); }
      catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 8000); }
    },
  );
  const finishCopy = async (r: Reward) => {
    try { const t = await api.rewardFinishManagedCopy(r.id); showNotification(`Название награды: «${t}»`, NOTIFICATION_TYPES.SUCCESS, 3000); setCopyInfo(null); void load(); }
    catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 8000); }
  };
  const openRedemptions = redemptions.filter((x) => x.status === "pending" || x.status === "refunded");
  const deleteOnTwitch = (r: Reward) => showConfirm(
    `Удалить награду «${r.rewardTitle}» на Twitch?\n\nЗрители её больше не увидят, невыполненные погашения останутся в очереди запросов Twitch. Реакция в боте тоже будет удалена. Это действие нельзя отменить.`,
    async () => {
      try { const t = await api.rewardDeleteTwitch(r.id); setEditing(null); showNotification(`Награда «${t}» удалена на Twitch`, NOTIFICATION_TYPES.WARNING, 4000); void load(); }
      catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 8000); }
    },
  );

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
          <div className={`eventsub-indicator ${es?.connected ? "connected" : running ? "connecting" : "disconnected"}`}>{es?.connected ? <><Icon name="status-connected" /> EventSub подключен</> : running ? <><Icon name="refresh" className="spinning" /> EventSub подключается…</> : <><Icon name="status-disconnected" /> EventSub отключен</>}</div>
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
                  {info && <span className="reward-cost">{info.cost} <Icon name="channel-points" /></span>}
                  {channel.length > 0 && !info && <Hint text={hintRewardMissing(r.rewardTitle)}><span className="reward-status-badge disabled">нет на канале</span></Hint>}
                  {info?.isManaged && <Hint text={<>награда <Em>«{r.rewardTitle}»</Em> создана через бота: бот может менять её и возвращать баллы зрителям</>}><span className="reward-status-badge managed"><Icon name="robot" /></span></Hint>}
                  {r.managed && r.originalRewardId && <Hint text={<>копия ещё с пометкой «(бот)»: удалите оригинал в панели Twitch и нажмите «Убрать пометку» в редакторе</>}><span className="reward-status-badge warning-badge">оригинал не удалён</span></Hint>}
                  {r.refundIfUnavailable && info?.isManaged && <Hint text={<>если оверлей выключен, бот вернёт баллы зрителю; удачные погашения бот закрывает сам</>}><span className="reward-status-badge refund"><Icon name="redo" /> возврат</span></Hint>}
                  <Hint text={hintReaction({ kind: "reward", name: r.rewardTitle }, r.response)}><span className="reward-type-badge">{reactionBadge(r.response)}</span></Hint>
                  {ov && <Hint text={hintOverlay(ov)}><span className="overlay-badge"><Icon name="overlay-screen" /> {ov.name}</span></Hint>}
                  {!ov && r.response.media.enabled && <Hint text={hintOverlayAll(config.overlays)}><span className="overlay-badge all-overlays"><Icon name="broadcast" /> Все оверлеи</span></Hint>}
                </div>
                <div className="reward-actions">
                  <Hint text={hintStatus({ kind: "reward", name: r.rewardTitle }, r.enabled)}><button onClick={() => { setSection("rewards", rewards.map((x) => (x.id === r.id ? { ...x, enabled: !x.enabled } : x))); }} className={`status-toggle-btn ${r.enabled ? "on" : "off"}`}><Icon name="power"  /></button></Hint>
                  <button onClick={() => setEditing({ reward: r, isNew: false })} className="edit-btn" title="Редактировать"><Icon name="edit"  /></button>
                  <button onClick={() => showConfirm(`Удалить реакцию на награду "${r.rewardTitle}"?`, () => { setSection("rewards", rewards.filter((x) => x.id !== r.id)); showNotification("Реакция удалена", NOTIFICATION_TYPES.WARNING, 2000); })} className="delete-btn" title="Удалить"><Icon name="delete"  /></button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
      <div className="add-reward-section">
        <button className="add-reward-main-btn" onClick={() => setCreating(defaultParams())} disabled={!running} title={running ? "Создать награду на канале от имени бота и сразу настроить реакцию" : "Бот не запущен — награду создать нельзя"}><Icon name="channel-points" /> Новая награда для Twitch</button>
        <button className="add-reward-main-btn" onClick={() => setEditing({ reward: null, isNew: true })}><Icon name="add"  /> Добавить реакцию</button>
      </div>

      <Modal isOpen={!!creating} onClose={() => setCreating(null)} title="Новая награда для Twitch" size="large">
        {creating && (
          <div>
            <ModalActions>
              <button className="primary" onClick={() => void createOnTwitch()} disabled={creatingBusy}><Icon name="channel-points" /> {creatingBusy ? "Создаём…" : "Создать на Twitch"}</button>
            </ModalActions>
            <p className="form-hint" style={{ marginBottom: 14 }}>Награда появится на канале сразу и будет создана через бота: ему доступны возврат баллов при недоступном оверлее и правка параметров отсюда. Картинку награде задают только в панели Twitch. Следующим шагом откроется редактор реакции.</p>
            <RewardParamsForm value={creating} onChange={setCreating} />
          </div>
        )}
      </Modal>

      <div className="redemptions-section">
        <div className="redemptions-header">
          <h3><Icon name="hourglass" /> Невыполненные погашения {openRedemptions.length > 0 && <span className="badge badge-warning">{openRedemptions.length}</span>}</h3>
          <div className="flex gap-2">
            <button className="small" onClick={() => void openQueue()} disabled={!running} title="Очередь запросов Twitch — там стример и модераторы возвращают баллы или отмечают выполнение"><Icon name="external-link" /> Очередь запросов Twitch</button>
          </div>
        </div>
        <p className="form-hint">Сюда попадают награды, чьё медиа не дошло до оверлея (он был выключен). Вернуть баллы зрителю можно в очереди запросов Twitch — это умеют стример и модераторы. Для наград, созданных через бота с включённым возвратом, бот делает это сам.</p>
        {openRedemptions.length === 0 ? <div className="form-hint">Пока пусто.</div> : (
          <div className="redemptions-list">
            {openRedemptions.map((x) => {
              const rw = rewards.find((r) => r.rewardId === x.rewardId);
              const managed = !!channel.find((c) => c.id === x.rewardId)?.isManaged;
              return (
                <div key={x.redemptionId} className={`redemption-row ${x.status}`}>
                  <div className="redemption-main">
                    <strong>{x.rewardTitle}</strong> — <span className="redemption-user">{x.user}</span>
                    <span className="redemption-time">{new Date(x.at).toLocaleString("ru-RU")}</span>
                    <div className="form-hint">{x.status === "refunded" ? "баллы возвращены ботом" : x.reason}</div>
                  </div>
                  <div className="redemption-actions">
                    {x.status === "pending" && managed && <button className="small" onClick={() => api.redemptionRefund(x.redemptionId).then(() => showNotification(`Баллы за «${x.rewardTitle}» возвращены ${x.user}`, NOTIFICATION_TYPES.SUCCESS, 3000)).catch((e) => showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 6000))}><Icon name="redo" /> Вернуть баллы</button>}
                    {x.status === "pending" && !managed && rw && <button className="small" onClick={() => void openQueue()} title="Награда создана в панели Twitch — вернуть баллы можно только там"><Icon name="external-link" /> В очереди Twitch</button>}
                    <button className="small" onClick={() => api.redemptionDismiss(x.redemptionId).then(loadRedemptions)} title="Убрать из списка"><Icon name="close" /></button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <Modal isOpen={!!copyInfo} onClose={() => setCopyInfo(null)} title="Копия награды создана" size="medium">
        {copyInfo && (
          <div className="managed-copy-steps">
            <p>На канале теперь две награды: <strong>«{copyInfo.result.originalTitle}»</strong> — оригинал, и <strong>«{copyInfo.result.newTitle}»</strong> — копия, созданная ботом. Реакция SignoreBot уже переведена на копию.</p>
            <ol>
              <li>Откройте <a href="#" onClick={(e) => { e.preventDefault(); void openUrl(copyInfo.result.rewardsUrl); }}>страницу наград в панели Twitch</a>.</li>
              <li>Найдите награду <strong>без</strong> пометки «(бот)» — это оригинал — и удалите её. Копию с пометкой «(бот)» не трогайте.</li>
              <li>Загрузите копии картинку награды: Twitch не принимает картинки от приложений, только через панель. Без неё у награды будет стандартный значок.</li>
              <li>Вернитесь сюда и нажмите «Убрать пометку»: копия получит прежнее название, зрители не заметят разницы. Если оригинал удалён при работающем боте, пометку он снимает сам.</li>
            </ol>
            <div className="flex gap-2">
              <button className="primary" onClick={() => void finishCopy(copyInfo.reward)}><Icon name="check" /> Убрать пометку «(бот)»</button>
              <button onClick={() => setCopyInfo(null)}>Позже</button>
            </div>
            <p className="form-hint" style={{ marginTop: 10 }}>Если нажать раньше, чем удалён оригинал, Twitch откажет: два одинаковых названия не допускаются.</p>
          </div>
        )}
      </Modal>
      <Modal isOpen={!!editing} onClose={() => setEditing(null)} size={editing?.reward ? "xlarge" : "medium"}
        title={!editing ? "" : !editing.reward ? "Выберите награду" : editing.isNew ? `Создание реакции на «${editing.reward.rewardTitle}»` : `Редактирование реакции на «${editing.reward.rewardTitle}»`}>
        {editing && !editing.reward && (
          <RewardSelector channel={channel} existing={rewards} loading={loading} onRefresh={() => void load()} onPick={(c) => setEditing({ reward: defaultReward(c.id, c.title), isNew: true })} onCancel={() => setEditing(null)} />
        )}
        {editing?.reward && <RewardEditor key={editing.reward.id} initial={editing.reward} isNew={editing.isNew} onSave={save} info={channel.find((c) => c.id === editing.reward!.rewardId)} onMakeCopy={(rw) => { save(rw); makeCopy(rw); }} onFinishCopy={(rw) => { save(rw); void finishCopy(rw); }} onChannelReload={() => void load()} onDeleteTwitch={(rw) => deleteOnTwitch(rw)} />}
      </Modal>
    </div>
  );
}

function RewardEditor({ initial, isNew, onSave, info, onMakeCopy, onFinishCopy, onChannelReload, onDeleteTwitch }: { initial: Reward; isNew: boolean; onSave: (r: Reward) => void; info?: ChannelReward; onMakeCopy: (r: Reward) => void; onFinishCopy: (r: Reward) => void; onChannelReload: () => void; onDeleteTwitch: (r: Reward) => void }) {
  const { config } = useAppState();
  const { showNotification } = useNotification();
  const [r, setR] = useState(initial);
  const managed = !!info?.isManaged;
  const [params, setParams] = useState<NewReward | null>(null);
  const [paramsBusy, setParamsBusy] = useState(false);
  const applyParams = async () => {
    if (!params) return;
    const problem = paramsProblem(params);
    if (problem) { showNotification(problem, NOTIFICATION_TYPES.WARNING, 3000); return; }
    setParamsBusy(true);
    try {
      const updated = await api.rewardUpdateTwitch(r.rewardId, params);
      setR({ ...r, rewardTitle: updated.title });
      setParams(paramsFromChannel(updated));
      showNotification(`Награда «${updated.title}» на Twitch обновлена`, NOTIFICATION_TYPES.SUCCESS, 3000);
      onChannelReload();
    } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 8000); }
    finally { setParamsBusy(false); }
  };
  return (
    <div className="reward-editor">
      {managed && info && (
        <details className="reward-params-block" open={isNew}>
          <summary><Icon name="channel-points" /> Параметры награды на Twitch <span className="form-hint" style={{ display: "inline", marginLeft: 8 }}>{info.cost} баллов{info.cooldownSeconds ? ` · кулдаун ${info.cooldownSeconds} с` : ""}{info.isEnabled ? "" : " · выключена"}</span></summary>
          <div className="reward-params-body">
            <RewardParamsForm value={params ?? paramsFromChannel(info)} onChange={setParams} />
            <div className="flex gap-2" style={{ marginTop: 8 }}>
              <button className="primary small" onClick={() => void applyParams()} disabled={paramsBusy || !params}><Icon name="check" /> {paramsBusy ? "Применяем…" : "Применить на Twitch"}</button>
              {params && <button className="small" onClick={() => setParams(null)}>Отменить правки</button>}
              <button className="small danger" style={{ marginLeft: "auto" }} onClick={() => onDeleteTwitch(r)} title="Убрать награду с канала и реакцию из бота"><Icon name="delete" /> Удалить награду на Twitch</button>
            </div>
            <div className="form-hint" style={{ marginTop: 8 }}>Те же настройки можно менять и в панели Twitch — бот подхватит их сам. Картинка — только там.</div>
          </div>
        </details>
      )}
      <div className="reward-refund-block">
        <label className={`toggle-label ${managed ? "" : "disabled"}`}>
          <span className="toggle-switch"><input type="checkbox" checked={r.refundIfUnavailable} disabled={!managed} onChange={(e) => setR({ ...r, refundIfUnavailable: e.target.checked })} /><span className="toggle-slider"></span></span>
          <span className="toggle-text">Возвращать баллы, если оверлей недоступен</span>
          <Tooltip text="Если медиа не дошло до оверлея (он был выключен), бот отменит погашение — Twitch вернёт зрителю баллы. Взамен бот сам закрывает удачные погашения, чтобы они не копились в очереди запросов. Работает только для наград, созданных через бота: Twitch разрешает это только приложению-создателю." />
        </label>
        {!managed && info && (
          <div className="form-hint">
            Награда создана в панели Twitch, а не через бота — Twitch не позволяет приложению отменять её погашения. Можно создать через бота копию с теми же параметрами и перевести реакцию на неё; бот подскажет, что удалить. Картинку награды придётся загрузить копии заново — её Twitch через приложение не принимает.
            <div style={{ marginTop: 8 }}><button className="small" onClick={() => onMakeCopy(r)}><Icon name="copy" /> Создать управляемую копию</button></div>
          </div>
        )}
        {managed && info?.skipQueue && <div className="form-hint text-warning"><Icon name="warning" /> У награды включено «пропускать очередь запросов»: такие погашения закрываются сразу, и вернуть баллы нельзя. Выключите этот пункт в настройках награды на Twitch.</div>}
        {r.managed && r.originalRewardId && <div className="form-hint"><Icon name="warning" /> Копия ещё с пометкой «(бот)». Удалите оригинал в панели Twitch и нажмите: <button className="small" onClick={() => onFinishCopy(r)}>Убрать пометку</button></div>}
        {!info && <div className="form-hint">Список наград канала недоступен (бот не запущен) — управление возвратом появится, когда бот подключится.</div>}
      </div>
      <ModalActions>
        <TestButton response={r.response} vars={{ user: "TestUser", message: "тест" }} />
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
