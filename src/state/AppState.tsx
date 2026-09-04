// Глобальное состояние панели: конфиг (с автосохранением по секциям) и
// статус ядра. Секция сохраняется целиком через `config_set_section`;
// правки в UI применяются оптимистично, ошибка сохранения — тост.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, errText, type ChangedWhat, type Config, type ConfigSection, type CoreStatus } from "../api";
import { useNotification, NOTIFICATION_TYPES } from "../components/Notification";

interface AppStateValue {
  config: Config;
  status: CoreStatus | null;
  /** Заменить секцию (и сохранить). */
  setSection: <K extends ConfigSection>(section: K, value: Config[K]) => void;
  /** Обновить секцию функцией от текущего значения. */
  updateSection: <K extends ConfigSection>(section: K, fn: (prev: Config[K]) => Config[K]) => void;
  reloadConfig: () => Promise<void>;
  reloadStatus: () => Promise<void>;
  /** Подписка на «что-то изменилось». */
  onChanged: (what: ChangedWhat, cb: () => void) => () => void;
}

const Ctx = createContext<AppStateValue | null>(null);

export function useAppState(): AppStateValue {
  const v = useContext(Ctx);
  if (!v) throw new Error("useAppState вне AppStateProvider");
  return v;
}

export function AppStateProvider({ children }: { children: ReactNode }) {
  const { showNotification } = useNotification();
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<CoreStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const listeners = useRef(new Map<ChangedWhat, Set<() => void>>());
  const pending = useRef(new Map<ConfigSection, ReturnType<typeof setTimeout>>());
  // Сколько наших собственных сохранений ещё не отозвалось событием "config":
  // такие события не должны перезагружать конфиг (иначе затрут правки соседних секций).
  const selfSaves = useRef(0);
  const configRef = useRef<Config | null>(null);
  configRef.current = config;

  const reloadConfig = useCallback(async () => {
    try {
      const c = await api.configGet();
      setConfig(c);
      setError(null);
    } catch (e) {
      setError(errText(e));
    }
  }, []);

  const reloadStatus = useCallback(async () => {
    try {
      setStatus(await api.status());
    } catch (e) {
      console.error("status", e);
    }
  }, []);

  useEffect(() => {
    void reloadConfig();
    void reloadStatus();
    let unlisten: (() => void) | undefined;
    void api.onChanged((what) => {
      if (what === "config") {
        if (selfSaves.current > 0) selfSaves.current -= 1;
        else void reloadConfig();
      }
      if (what === "auth" || what === "server" || what === "overlays" || what === "eventsub" || what === "runtime" || what === "updates") void reloadStatus();
      listeners.current.get(what)?.forEach((cb) => cb());
    }).then((u) => (unlisten = u));
    const t = setInterval(() => void reloadStatus(), 15000);
    return () => {
      unlisten?.();
      clearInterval(t);
    };
  }, [reloadConfig, reloadStatus]);

  const flush = useCallback(
    (section: ConfigSection) => {
      const cur = configRef.current;
      if (!cur) return;
      selfSaves.current += 1;
      api.configSetSection(section, cur[section]).then((saved) => {
        // Ядро могло нормализовать данные (регистр имён, пути); подхватываем,
        // если других незавершённых правок нет.
        if (pending.current.size === 0) setConfig(saved);
      }).catch((e) => {
        selfSaves.current = Math.max(0, selfSaves.current - 1);
        showNotification(`Не удалось сохранить «${section}»: ${errText(e)}`, NOTIFICATION_TYPES.ERROR, 5000);
        void reloadConfig();
      });
    },
    [reloadConfig, showNotification],
  );

  const setSection = useCallback(
    <K extends ConfigSection>(section: K, value: Config[K]) => {
      setConfig((prev) => (prev ? { ...prev, [section]: value } : prev));
      const old = pending.current.get(section);
      if (old) clearTimeout(old);
      pending.current.set(
        section,
        setTimeout(() => {
          pending.current.delete(section);
          flush(section);
        }, 250),
      );
    },
    [flush],
  );

  const updateSection = useCallback(
    <K extends ConfigSection>(section: K, fn: (prev: Config[K]) => Config[K]) => {
      const cur = configRef.current;
      if (!cur) return;
      setSection(section, fn(cur[section]));
    },
    [setSection],
  );

  const onChanged = useCallback((what: ChangedWhat, cb: () => void) => {
    let set = listeners.current.get(what);
    if (!set) {
      set = new Set();
      listeners.current.set(what, set);
    }
    set.add(cb);
    return () => {
      set?.delete(cb);
    };
  }, []);

  const value = useMemo<AppStateValue | null>(
    () => (config ? { config, status, setSection, updateSection, reloadConfig, reloadStatus, onChanged } : null),
    [config, status, setSection, updateSection, reloadConfig, reloadStatus, onChanged],
  );

  if (!value) {
    return (
      <div className="loading-screen">
        <div className="loading-spinner"></div>
        <p>{error ? `Ошибка загрузки конфигурации: ${error}` : "Загрузка конфигурации..."}</p>
        {error && <button onClick={() => void reloadConfig()}>Повторить</button>}
      </div>
    );
  }
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
