// Кнопка «Тест» в шапке редактора: выполняет реакцию как она настроена
// сейчас (без сохранения) — текст в чат и медиа на оверлей — и говорит,
// что из этого получилось.

import Icon from "../Icon";
import { api, errText, type Response } from "../../api";
import { useNotification, NOTIFICATION_TYPES } from "../Notification";

export default function TestButton({ response, vars, eventType }: { response: Response; vars?: Record<string, string>; eventType?: string }) {
  const { showNotification } = useNotification();
  const run = async () => {
    try {
      const r = await api.responseTest(response, vars ?? {}, eventType ?? null);
      const parts: string[] = [];
      let ok = true;
      if (r.chatEnabled) {
        if (r.chatSent) parts.push("текст ушёл в чат");
        else { ok = false; parts.push(r.running ? "текст в чат не отправлен (см. логи)" : "текст в чат не отправлен — бот не запущен"); }
      }
      if (r.mediaEnabled) {
        if (r.mediaSent) parts.push("медиа показано на оверлее");
        else if (r.mediaUnavailable) { ok = false; parts.push("оверлей не подключён — медиа ждёт в очереди или сработал резерв"); }
        else { ok = false; parts.push("медиа не отправлено"); }
      }
      if (!r.chatEnabled && !r.mediaEnabled) { ok = false; parts.push("реакция пуста — включите текст в чат или медиа"); }
      showNotification(`Тест: ${parts.join("; ")}`, ok ? NOTIFICATION_TYPES.SUCCESS : NOTIFICATION_TYPES.WARNING, 5000);
    } catch (e) { showNotification(errText(e), NOTIFICATION_TYPES.ERROR, 6000); }
  };
  return <button onClick={() => void run()} className="modal-test-btn" title="Выполнить реакцию прямо сейчас: текст в чат и медиа на оверлей (с тестовыми значениями переменных)"><Icon name="play" /> Тест</button>;
}
