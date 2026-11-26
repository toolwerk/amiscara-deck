// Texts of the panel: English in the code, other languages from po/de.po,
// which the backend hands over once (`catalog`). `scripts/i18n.py extract`
// finds every t("...") here as it does in Rust.
import { invoke } from "@tauri-apps/api/core";

let texts: Record<string, string> = {};

export async function loadTexts(): Promise<void> {
  try {
    texts = await invoke<Record<string, string>>("catalog", { locale: navigator.language });
  } catch {
    texts = {};
  }
}

/// The text in the language of the system, with {placeholders} filled.
export function t(msgid: string, args: Record<string, string | number> = {}): string {
  const text = texts[msgid] ?? msgid;
  return text.replace(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g, (all, name: string) =>
    name in args ? String(args[name]) : all,
  );
}
