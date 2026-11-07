// Подключение канала и выбор отправителя. Попытка входа отделена от
// действующей авторизации: ядро принимает только проверенный Twitch ID.

import { useRef, useState, type MutableRefObject } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText, type AccountKind, type AccountStatus } from "../../api";
import { copyText } from "../../api/clipboard";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Icon from "../Icon";
import Tooltip from "../Tooltip";
import { Hint } from "../Common/hints";
import "./OAuthTab.css";

const SCOPE_LABELS: Record<string, string> = {
  "user:read:chat": "чтение чата",
  "user:write:chat": "отправка сообщений",
  "moderator:manage:chat_messages": "удаление сообщений",
  "moderator:manage:shoutouts": "шатауты",
  "moderator:read:shoutouts": "просмотр шатаутов",
  "moderator:read:chatters": "список зрителей",
  "moderator:read:followers": "фолловеры",
  "channel:read:subscriptions": "подписки",
  "channel:read:hype_train": "хайповоз",
  "channel:read:redemptions": "награды за баллы",
  "channel:manage:redemptions": "возврат баллов",
  "bits:read": "bits",
};

const connected = (st: AccountStatus) => st.state === "authorized" && st.missingScopes.length === 0;

function AccountCard({ kind, st, streamer, shared, locked, otherPending, changingMode, duplicate, onShared, revision }: {
  kind: AccountKind;
  st: AccountStatus;
  streamer: string | null;
  shared: boolean;
  locked?: boolean;
  otherPending: boolean;
  changingMode: boolean;
  duplicate?: boolean;
  onShared: () => Promise<void>;
  revision: MutableRefObject<number>;
}) {
  const { reloadStatus } = useAppState();
  const { showNotification, showConfirm } = useNotification();
  const [busy, setBusy] = useState(false);
  const [copiedFor, setCopiedFor] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const bot = kind === "bot";
  const ready = connected(st) && !duplicate;
  const attempt = st.attempt;
  const device = attempt?.device;
  const conflict = attempt?.conflictingLogin || (duplicate ? st.login : null);
  const loginDisabled = busy || changingMode || otherPending;
  const managementDisabled = busy || changingMode;

  const start = async (destination: "browser" | "copy") => {
    const ticket = ++revision.current;
    setBusy(true);
    setActionError(null);
    setCopiedFor(null);
    try {
      const dc = await api.authStart(kind);
      if (ticket !== revision.current) return;
      // Сначала показываем код, чтобы ошибка браузера/буфера не спрятала вход.
      await reloadStatus();
      if (ticket !== revision.current) return;
      if (destination === "browser") await openUrl(dc.verificationUri);
      if (destination === "copy") {
        await copyText(dc.verificationUri);
        if (ticket === revision.current) setCopiedFor(dc.deviceCode);
      }
    } catch (e) {
      if (ticket === revision.current) setActionError(errText(e));
    } finally { setBusy(false); }
  };

  const copy = async (text: string, link = false) => {
    setActionError(null);
    try {
      await copyText(text);
      if (link && device) setCopiedFor(device.deviceCode);
      else showNotification("Код скопирован", NOTIFICATION_TYPES.INFO, 1500);
    } catch (e) { setActionError(`Не удалось скопировать: ${errText(e)}. Ссылку и код можно выделить вручную.`); }
  };

  const cancel = async () => {
    revision.current += 1;
    setActionError(null);
    try { await api.authCancel(kind); await reloadStatus(); }
    catch (e) { setActionError(errText(e)); }
  };

  const refresh = async () => {
    setBusy(true);
    try { await api.authRefresh(kind); await reloadStatus(); }
    catch (e) { setActionError(`Не удалось обновить токен: ${errText(e)}`); }
    finally { setBusy(false); }
  };

  const logout = () => showConfirm(
    bot ? "Удалить авторизацию бота? Бот перестанет писать в чат до повторного входа."
      : "Удалить авторизацию стримера? Бот перестанет получать события канала и читать чат до повторного входа.",
    () => {
      revision.current += 1;
      setBusy(true);
      void api.authLogout(kind).then(reloadStatus).catch((e) => setActionError(errText(e))).finally(() => setBusy(false));
    },
  );

  const openBrowser = async () => {
    setActionError(null);
    try { await openUrl(device!.verificationUri); }
    catch (e) { setActionError(`Не удалось открыть браузер: ${errText(e)}`); }
  };
  const loginButtons = (retry = false) => <>
    {!bot && <button className="oauth-primary" disabled={loginDisabled} onClick={() => void (device ? openBrowser() : start("browser"))}><Icon name="external-link" /> Открыть в браузере</button>}
    <button className={bot ? "oauth-primary" : "oauth-secondary"} disabled={loginDisabled} onClick={() => void (device ? copy(device.verificationUri, true) : start("copy"))}><Icon name="copy" /> {retry && bot ? "Повторить вход для бота" : "Скопировать ссылку"}</button>
  </>;

  return (
    <section className={`oauth-card${locked ? " oauth-card-locked" : ""}`} aria-labelledby={`oauth-${kind}-title`}>
      <div className="oauth-card-heading">
        <span className="oauth-role-icon"><Icon name={bot ? "robot" : "video-camera"} /></span>
        <div>
          <h3 id={`oauth-${kind}-title`}>{bot ? "2. Аккаунт бота" : shared ? "Канал стримера" : "1. Канал стримера"}</h3>
          <p>{bot ? "Ответы в чате от имени бота" : shared ? "События канала, чтение чата и ответы от имени стримера" : "События канала и чтение чата"}</p>
        </div>
        {ready && <span className="oauth-connected"><Hint text="Twitch подтвердил аккаунт и необходимые для этой роли разрешения"><span><Icon name="circle-check" /> Подключён</span></Hint></span>}
      </div>

      {st.login && <div className="oauth-account-line">
        <strong>@{st.login}</strong>
        <Tooltip text={<>
          <div><strong>Twitch ID:</strong> {st.userId ?? "ещё не проверен"}</div>
          <div><strong>Права:</strong> {st.scopes.map((scope) => SCOPE_LABELS[scope] ?? scope).join(", ") || "ещё не проверены"}.</div>
          <div>Токен обновляется автоматически.</div>
        </>} />
      </div>}
      {locked ? <p className="oauth-muted">Сначала авторизуйте аккаунт стримера.</p> : <>
        {device ? (
          <div className="oauth-login-stage" aria-live="polite">
            <h4><Icon name="hourglass" /> Подтвердите вход на Twitch</h4>
            {bot && <p>Откройте ссылку в браузере или профиле с аккаунтом бота, отличным от <strong>@{streamer}</strong>.</p>}
            <div className="oauth-device-code"><code>{device.userCode}</code><button className="oauth-link-btn" disabled={busy} onClick={() => void copy(device.userCode)} aria-label="Скопировать код"><Icon name="copy" /> Код</button></div>
            <div className="oauth-actions">
              {loginButtons()}
              <button className="oauth-secondary" onClick={() => void cancel()}>Отмена</button>
            </div>
            <p className="oauth-pending" role="status">{copiedFor === device.deviceCode ? "Ссылка скопирована. Вход ещё не завершён" : "Ожидание подтверждения на Twitch"}</p>
            {actionError && <p className="oauth-manual-link">{device.verificationUri}</p>}
          </div>
        ) : conflict ? (
          <div className="oauth-problem" role="alert">
            <h4><Icon name="triangle-alert" /> {bot ? "Этот аккаунт уже подключён как стример" : "Этот аккаунт уже подключён как бот"}</h4>
            <p>{duplicate ? "Обе роли сохранены с одним Twitch ID." : <>Twitch вернул <strong>@{conflict}</strong>.</>} Для отдельного бота нужен другой аккаунт.</p>
            <div className="oauth-actions">
              {loginButtons(true)}
              <button className="oauth-secondary" disabled={managementDisabled} onClick={() => void onShared()}>Писать от имени @{streamer ?? conflict}</button>
            </div>
          </div>
        ) : attempt?.error ? (
          <div className="oauth-problem" role="alert">
            <h4><Icon name="triangle-alert" /> Вход не завершён</h4><p>{attempt.error}</p>
            <div className="oauth-actions">{loginButtons()}</div>
          </div>
        ) : !ready ? (
          <div className="oauth-connect">
            {st.state === "invalid" && <p className="oauth-problem-text"><Icon name="user-x" /> {st.error ?? "Вход больше не действует. Авторизуйтесь заново."}</p>}
            {st.state === "authorized" && st.missingScopes.length > 0 && <p className="oauth-problem-text">Нужно заново подтвердить права Twitch: {st.missingScopes.map((scope) => SCOPE_LABELS[scope] ?? scope).join(", ")}.</p>}
            <div className="oauth-actions">{loginButtons()}</div>
          </div>
        ) : null}
      </>}
      {actionError && <p className="oauth-problem-text" role="alert">{actionError}</p>}
      {st.error && st.state !== "invalid" && <p className="oauth-problem-text">{st.error}</p>}
      {st.login && <div className="oauth-actions oauth-account-actions">
        <button className="oauth-secondary" disabled={managementDisabled} onClick={() => void refresh()}><Icon name="refresh" /> Обновить токен</button>
        <button className="oauth-secondary oauth-danger" disabled={managementDisabled} onClick={logout}><Icon name="log-out" /> Выйти</button>
      </div>}
    </section>
  );
}

export default function AuthTab() {
  const { status, reloadStatus, reloadConfig } = useAppState();
  const { showNotification } = useNotification();
  const [changingMode, setChangingMode] = useState(false);
  const revision = useRef(0);
  if (!status) return null;
  const setup = status.authSetup;
  const same = setup.selectedSameAccount;
  const streamer = setup.broadcaster;
  const bot = setup.bot;
  const streamerReady = connected(streamer);
  const switching = same !== setup.sameAccount;
  const ready = same ? streamerReady && setup.sameAccount : streamerReady && connected(bot) && !setup.duplicateAccounts && !setup.sameAccount;
  const sender = setup.sameAccount ? streamer.login : status.bot.login;

  const chooseMode = async (on: boolean) => {
    revision.current += 1;
    setChangingMode(true);
    try { await api.authSetSameAccount(on); await Promise.all([reloadStatus(), reloadConfig()]); }
    catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setChangingMode(false); }
  };

  return (
    <div className="oauth-tab">
      <header className="oauth-header"><h2><Icon name="lock-key" /> Авторизация Twitch</h2><p>Подключите канал и выберите, от чьего имени будут приходить ответы в чат.</p></header>
      <fieldset className="oauth-modes" disabled={changingMode}>
        <legend>От чьего имени бот должен писать в чат?</legend>
        <div className="oauth-mode-options">
          <label className={`oauth-mode${same ? " selected" : ""}`}>
            <input type="radio" name="oauth-mode" value="shared" checked={same} onChange={() => void chooseMode(true)} />
            <span className="oauth-mode-icon"><Icon name="user" /></span><span><strong>От имени стримера</strong><span>Если использовать один аккаунт</span></span>
          </label>
          <label className={`oauth-mode${!same ? " selected" : ""}`}>
            <input type="radio" name="oauth-mode" value="separate" checked={!same} onChange={() => void chooseMode(false)} />
            <span className="oauth-mode-icon"><Icon name="robot" /></span><span><strong>От отдельного имени</strong><span>Если использовать два разных аккаунта</span></span>
          </label>
        </div>
      </fieldset>
      {switching && <div className="oauth-transition" role="status"><Icon name="circle-info" /><div>
        {status.running && sender ? <>Сейчас бот пишет как <strong>@{sender}</strong>. </> : null}
        {same ? "Чтобы писать от имени стримера, подтвердите права канала заново." : "Отправитель сменится после подключения другого аккаунта бота."}
      </div></div>}
      <div className="oauth-steps">
        <AccountCard kind="broadcaster" st={streamer} streamer={streamer.login} shared={same} changingMode={changingMode} otherPending={!!bot.attempt?.device} onShared={() => chooseMode(true)} revision={revision} />
        {!same && <AccountCard kind="bot" st={bot} streamer={streamer.login} shared={false} locked={!streamerReady} changingMode={changingMode} otherPending={!!streamer.attempt?.device} duplicate={setup.duplicateAccounts} onShared={() => chooseMode(true)} revision={revision} />}
      </div>
      {ready && <div className="oauth-sender" role="status"><Icon name="circle-check" /><div><strong>Бот пишет как @{same ? streamer.login : bot.login}</strong><p>{same ? "Общий аккаунт · Один вход для двух ролей" : "Отдельный аккаунт · Канал и бот подключены"}</p></div></div>}
      <details className="oauth-info">
        <summary><span>Как работает вход</span><Icon name="chevron-right" className="oauth-info-chevron" /></summary>
        <p>Откройте ссылку, введите код на Twitch и подтвердите права. Перед подтверждением проверьте ник аккаунта.</p>
        <p>Ссылка открывает тот аккаунт, который выбран в браузере. Для отдельного бота используйте другой профиль или браузер. Копирование ссылки ещё не завершает вход.</p>
        <p>Токены хранятся в {status.secretsBackend === "keyring" ? "системном хранилище" : "файле secrets.json в каталоге данных"} и обновляются автоматически. Бот запускается после авторизации. Выход стримера отменяет ожидающий вход бота.</p>
        <p>Для удаления сообщений отдельный бот должен быть модератором канала.</p>
      </details>
    </div>
  );
}
