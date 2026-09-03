// Авторизация двух аккаунтов через Twitch Device Code Flow.

import Icon from "../Icon";
import type { ReactNode } from "react";
import { copyText } from "../../api/clipboard";
import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText, type AccountKind, type AccountStatus } from "../../api";
import { useAppState } from "../../state/AppState";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";
import Tooltip from "../Tooltip";
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
  "channel:read:redemptions": "награды за баллы",
  "bits:read": "bits",
};

function AccountCard({ kind, st, title, badge, hint }: { kind: AccountKind; st: AccountStatus; title: ReactNode; badge: string; hint: string }) {
  const { showNotification, showConfirm } = useNotification();
  const [busy, setBusy] = useState(false);
  const label = kind === "broadcaster" ? "стримера" : "бота";

  // openBrowser=false — только скопировать ссылку (для другого браузера/профиля)
  const start = async (openBrowser: boolean) => {
    setBusy(true);
    try {
      const dc = await api.authStart(kind);
      if (openBrowser) {
        showNotification(`Введите код ${dc.userCode} на странице Twitch`, NOTIFICATION_TYPES.INFO, 4000);
        await openUrl(dc.verificationUri).catch(() => {});
      } else {
        await copyText(dc.verificationUri);
        showNotification(`Ссылка с кодом ${dc.userCode} скопирована — откройте её в браузере, где залогинен нужный аккаунт`, NOTIFICATION_TYPES.SUCCESS, 6000);
      }
    } catch (e) { showNotification(`${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setBusy(false); }
  };
  const refresh = async () => {
    setBusy(true);
    try { await api.authRefresh(kind); showNotification(`Токен ${label} обновлён`, NOTIFICATION_TYPES.SUCCESS, 2000); }
    catch (e) { showNotification(`Не удалось обновить токен ${label}: ${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000); }
    finally { setBusy(false); }
  };
  const logout = () => showConfirm(
    kind === "broadcaster"
      ? "Удалить авторизацию стримера?\n\nБот перестанет получать события канала и читать чат до повторной авторизации."
      : "Удалить авторизацию бота?\n\nБот перестанет писать в чат и удалять сообщения до повторной авторизации.",
    () => { void api.authLogout(kind).then(() => showNotification(`Авторизация ${label} удалена`, NOTIFICATION_TYPES.SUCCESS, 2000)); },
  );

  const scopesText = st.scopes.map((s) => SCOPE_LABELS[s] ?? s).join(", ");
  const expires = st.expiresAt ? Math.max(0, Math.round((st.expiresAt * 1000 - Date.now()) / 60000)) : null;

  return (
    <div className="oauth-card">
      <div className="oauth-card-header"><h3>{title}</h3><span className={`oauth-badge ${kind}`}>{badge}</span></div>
      <div className="oauth-card-content">
        {st.state === "pending" && st.device ? (
          <div className="oauth-auth-status loading">
            <div>
              <strong>Ожидание подтверждения в браузере</strong>
              <div className="oauth-scopes">Откройте <a href="#" onClick={(e) => { e.preventDefault(); void openUrl(st.device!.verificationUri); }}>{st.device.verificationUri}</a> и введите код:</div>
              <div style={{ fontSize: 28, fontFamily: "monospace", letterSpacing: 4, margin: "8px 0", userSelect: "all" }}>{st.device.userCode}</div>
              <div className="oauth-actions">
                <button className="oauth-copy-btn" onClick={() => { void copyText(st.device!.verificationUri); showNotification("Ссылка с кодом скопирована", NOTIFICATION_TYPES.SUCCESS, 1500); }}><Icon name="copy"  /> Скопировать ссылку</button>
                <button className="oauth-copy-btn" onClick={() => { void copyText(st.device!.userCode); showNotification("Код скопирован", NOTIFICATION_TYPES.SUCCESS, 1500); }}><Icon name="copy"  /> Код</button>
                <button className="oauth-logout-btn" onClick={() => void api.authCancel(kind)}>Отмена</button>
              </div>
            </div>
          </div>
        ) : st.state === "authorized" ? (
          <div className="oauth-auth-status success">
            <Icon name="success-badge"  />
            <div>
              <strong>{st.login}</strong>
              <div className="oauth-scopes">
                Прав: {st.scopes.length}{st.scopes.length > 0 && <Tooltip text={scopesText} />}
                {expires !== null && <> · обновляется автоматически{expires <= 10 ? " (сейчас)" : ""} <Tooltip text={`Токены Twitch живут около 4 часов; бот сам обновляет их за 5 минут до истечения (следующее обновление примерно через ${expires} мин) и при любом ответе 401. Авторизоваться заново не нужно — даже на 9-часовом стриме.`} /></>}
              </div>
              {st.missingScopes.length > 0 && <div className="oauth-scopes text-warning"><Icon name="warning" /> Не хватает прав: {st.missingScopes.map((s) => SCOPE_LABELS[s] ?? s).join(", ")} — авторизуйтесь заново</div>}
            </div>
          </div>
        ) : st.state === "invalid" ? (
          <div className="oauth-auth-status error"><Icon name="warning"  /><div><strong>Токен недействителен</strong><div className="oauth-scopes">{st.error ?? "Повторите авторизацию"}</div></div></div>
        ) : (
          <div className="oauth-auth-status muted"><div>Не авторизован{st.error && <div className="oauth-scopes text-danger">{st.error}</div>}</div></div>
        )}

        {st.state !== "pending" && (
          <div className="oauth-actions">
            {st.state === "authorized" ? (
              <>
                {st.missingScopes.length > 0 && <button onClick={() => void start(true)} className="oauth-auth-btn" disabled={busy}><Icon name="auth-lock" /> Авторизоваться заново</button>}
                <button onClick={() => void refresh()} className="oauth-refresh-btn" disabled={busy} title="Обновить токен"><Icon name="refresh" className={busy ? "spinning" : ""} /> Обновить токен</button>
                <button onClick={logout} className="oauth-logout-btn" title="Удалить авторизацию"><Icon name="sign-out"  /> Выйти</button>
              </>
            ) : (
              <>
                <button onClick={() => void start(true)} className="oauth-auth-btn" disabled={busy}><Icon name="auth-lock" /> Авторизоваться</button>
                <button onClick={() => void start(false)} className="oauth-copy-btn" disabled={busy} title="Не открывать браузер, а скопировать ссылку с кодом — для другого браузера или профиля"><Icon name="copy"  /> Скопировать ссылку</button>
                {st.state === "invalid" && <button onClick={logout} className="oauth-logout-btn"><Icon name="sign-out"  /> Забыть</button>}
              </>
            )}
          </div>
        )}
        <div className="oauth-hint"><Icon name="lightbulb" /> {hint}</div>
      </div>
    </div>
  );
}

export default function AuthTab() {
  const { status } = useAppState();
  if (!status) return null;
  return (
    <div className="oauth-tab">
      <div className="oauth-header">
        <h2><Icon name="auth-lock" /> Авторизация Twitch</h2>
        <p className="oauth-description">Нужны два аккаунта: стример (события канала и чтение чата) и бот (сообщения в чат, удаление сообщений). Авторизация — по коду на странице Twitch, без ввода паролей в приложении.</p>
      </div>
      <div className="oauth-two-columns">
        <AccountCard kind="broadcaster" st={status.broadcaster} title={<><Icon name="streamer-camera" /> Стример</>} badge="Основной аккаунт" hint="Права: чтение чата, фолловеры, подписки, награды за баллы, bits, список зрителей, shoutout." />
        <AccountCard kind="bot" st={status.bot} title={<><Icon name="robot" /> Бот</>} badge="Аккаунт для чата" hint="Права: отправка сообщений и удаление сообщений. Бот должен быть модератором канала, чтобы удалять сообщения." />
      </div>
      <div className="oauth-info">
        <h4><Icon name="pin" /> Как это работает</h4>
        <ul>
          <li>Нажмите «Авторизоваться» — откроется страница Twitch с полем для кода. Введите показанный код и подтвердите права.</li>
          <li>Если бот и стример — разные аккаунты в разных браузерах, нажмите «Скопировать ссылку» и вставьте её в браузер, где залогинен нужный аккаунт (код уже внутри ссылки).</li>
          <li>Токены хранятся в системном хранилище ({status.secretsBackend === "keyring" ? "keyring" : "файл secrets.json"}) и обновляются автоматически. Перезапуск не нужен: бот запускается сам, как только оба аккаунта готовы.</li>
          <li>Twitch отзывает refresh-токен после 30 дней без запусков — тогда просто авторизуйтесь заново.</li>
          <li>Приложение работает в одном экземпляре: повторный запуск (например, новой сборки при живой старой в трее) просто покажет уже открытое окно. Две копии по очереди «протухали» бы токены друг друга.</li>
        </ul>
      </div>
    </div>
  );
}
