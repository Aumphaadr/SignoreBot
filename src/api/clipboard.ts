// Копирование в буфер обмена. `navigator.clipboard` в WebKit работает только
// внутри «свежего» клика — после await IPC он бросает NotAllowedError
// («The request is not allowed by the user agent…»). Плагин Tauri пишет в
// системный буфер напрямую, без этого ограничения.

import { writeText } from "@tauri-apps/plugin-clipboard-manager";

export async function copyText(text: string): Promise<void> {
  try {
    await writeText(text);
  } catch {
    await navigator.clipboard.writeText(text);
  }
}
