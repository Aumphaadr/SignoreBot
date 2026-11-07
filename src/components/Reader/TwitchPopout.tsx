import { useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, errText } from "../../api";
import Icon from "../Icon";

// Keep old effect cleanup and the next effect's show command ordered across mode changes.
let nativeQueue = Promise.resolve();
function updateView(bounds: Parameters<typeof api.twitchView>[0], reload = false) {
  const operation = nativeQueue.catch(() => {}).then(() => api.twitchView(bounds, reload));
  nativeQueue = operation; return operation;
}

/** The child webview sits above DOM layers. Hide it before any app dialog is shown. */
export default function TwitchPopout({ active, login, zoom }: { active: boolean; login: string | null; zoom: number }) {
  const host = useRef<HTMLDivElement>(null);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    const el = host.current;
    let disposed = false, running = false, dirty = false, reload = retry > 0;
    const sync = async () => {
      dirty = true; if (running) return; running = true;
      while (dirty && !disposed) {
        dirty = false;
        const rect = el?.getBoundingClientRect();
        const blocked = !!document.querySelector(".modal-overlay, .confirm-overlay");
        const show = active && login && rect && rect.width >= 100 && rect.height >= 100 && !blocked;
        const ratio = window.devicePixelRatio;
        try {
          await updateView(show ? { x: rect.x * ratio, y: rect.y * ratio, width: rect.width * ratio, height: rect.height * ratio } : null, reload);
          reload = false;
          if (!disposed) setError("");
        } catch (e) { if (!disposed) setError(errText(e)); }
      }
      running = false;
    };
    const update = () => void sync();
    const resize = new ResizeObserver(update); if (el) resize.observe(el);
    // Only additions/removals matter; observing attributes would loop on Twitch placeholder styles.
    const dialogs = new MutationObserver(update); dialogs.observe(document.body, { childList: true, subtree: true });
    window.addEventListener("resize", update);
    document.addEventListener("scroll", update, true);
    update();
    return () => { disposed = true; resize.disconnect(); dialogs.disconnect(); window.removeEventListener("resize", update); document.removeEventListener("scroll", update, true); void updateView(null).catch(() => {}); };
  }, [active, login, zoom, retry]);
  return <div className="reader-native">
    <div className="reader-native-toolbar"><span>Стандартный чат Twitch · вход выполняется отдельно</span><button onClick={() => setRetry((n) => n + 1)} title="Перезагрузить Twitch Popout"><Icon name="refresh" /></button>{login && <button onClick={() => void openUrl(`https://www.twitch.tv/popout/${encodeURIComponent(login)}/chat?popout=`)}><Icon name="external-link" /> Открыть в браузере</button>}</div>
    <div className="reader-native-host" ref={host}>{!login ? <p>Авторизуйте стримера в разделе «Авторизация».</p> : error ? <div className="reader-native-error"><p>Не удалось встроить Twitch: {error}</p><button onClick={() => setRetry((n) => n + 1)}>Повторить</button></div> : <p>Загрузка Twitch…<br /><small>Если страница не открылась, попробуйте перезагрузить её или открыть в браузере.</small></p>}</div>
  </div>;
}
