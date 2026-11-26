// The shape of the state the backend sends (`deck_proto::agent::Agent`) and
// how the panel words it, with the same texts as the Mac app.
import { t } from "./i18n";

export type Status = "working" | "blocked" | "done" | "idle" | "error" | "exited";

export type Activity =
  | { kind: "run"; command: string | null }
  | { kind: "edit"; path: string | null }
  | { kind: "read"; path: string | null }
  | { kind: "search"; pattern: string | null }
  | { kind: "subagent" }
  | { kind: "web" }
  | { kind: "compacting" }
  | { kind: "tool"; name: string };

export interface Request {
  id: string;
  tool_name: string;
  summary: string;
  truncated: boolean;
  full: string;
  /// Lines of `full` (from 0) that are section headings such as `--- old ---`.
  headings: number[];
  extra: string[];
  state: "held" | "passed_through" | "answered";
  questions: unknown[];
}

export type HistoryEvent =
  | { event: "session_started" }
  | { event: "prompt_received" }
  | ({ event: "activity" } & Activity)
  | { event: "permission_requested" }
  | { event: "compacting" }
  | { event: "turn_ended" }
  | { event: "turn_failed" }
  | { event: "session_ended" }
  | { event: "allowed" }
  | { event: "denied" }
  | { event: "answered" };

export interface HistoryEntry {
  at_ms: number;
  event: HistoryEvent;
}

export interface Agent {
  id: string;
  host: string;
  tool: "claude" | "codex";
  project: string | null;
  branch: string | null;
  title: string | null;
  status: Status;
  status_since_ms: number;
  activity: Activity | null;
  last_prompt: string | null;
  context: { used_tokens: number; window_tokens: number } | null;
  turns: number | null;
  subagents: number;
  request: Request | null;
  history: HistoryEntry[];
  cost_micro_usd: number | null;
}

export interface View {
  connected: boolean;
  problem: string | null;
  agents: Agent[];
  /// What today cost on this machine, in millionths of a US dollar (estimate).
  today_micro_usd: number | null;
}

export function displayName(a: Agent): string {
  return a.project ?? a.title ?? a.id;
}

export function activityText(a: Activity): string {
  switch (a.kind) {
    case "run":
      return a.command ? t("running: {command}", { command: a.command }) : t("running a command");
    case "edit":
      return a.path ? t("editing {path}", { path: a.path }) : t("editing a file");
    case "read":
      return a.path ? t("reading {path}", { path: a.path }) : t("reading a file");
    case "search":
      return a.pattern ? t("searching {pattern}", { pattern: a.pattern }) : t("searching");
    case "subagent":
      return t("starting a subagent");
    case "web":
      return t("fetching from the web");
    case "compacting":
      return t("compacting");
    case "tool":
      return a.name;
  }
}

export function statusText(s: Status): string {
  switch (s) {
    case "working":
      return t("working");
    case "blocked":
      return t("waiting");
    case "done":
      return t("done");
    case "idle":
      return t("no task");
    case "error":
      return t("error");
    case "exited":
      return t("ended");
  }
}

/// "main · editing src/session.rs": branch, then what it does or did last.
export function detailLine(a: Agent): string {
  let what: string | null;
  const activity = a.activity ? activityText(a.activity) : null;
  switch (a.status) {
    case "working":
    case "error":
      what = activity ?? a.last_prompt;
      break;
    case "exited":
      what = t("Session ended");
      break;
    default:
      what = a.last_prompt;
  }
  return [a.branch, what].filter((x) => x).join(" · ");
}

export function requestTitle(tool: string): string {
  switch (tool) {
    case "Bash":
      return t("Wants to run a command");
    case "Write":
      return t("Wants to write a file");
    case "Edit":
    case "MultiEdit":
      return t("Wants to change a file");
    case "NotebookEdit":
      return t("Wants to change a notebook");
    case "Read":
      return t("Wants to read a file");
    case "WebFetch":
      return t("Wants to fetch a web page");
    case "WebSearch":
      return t("Wants to search the web");
    case "AskUserQuestion":
      return t("Has a question for you");
    case "ExitPlanMode":
      return t("Presents a plan for approval");
    default:
      return t("Wants to use {tool}", { tool });
  }
}

/// A section name of the daemon ("content", "2 old") in the language of the panel.
function section(name: string): string | null {
  const m = /^(\d+) (.+)$/.exec(name);
  const word = m ? m[2] : name;
  let text: string;
  switch (word) {
    case "content":
      text = t("content");
      break;
    case "old":
      text = t("old");
      break;
    case "new":
      text = t("new");
      break;
    case "options":
      text = t("options");
      break;
    default:
      return null;
  }
  return m ? `${m[1]} ${text}` : text;
}

/// The whole text as the Mac app shows it: only the lines the daemon marked
/// as headings are translated, the content stays word for word; then the
/// fields the card does not name.
export function fullText(r: Request): string {
  const marked = new Set(r.headings ?? []);
  const lines = r.full.split("\n").map((line, i) => {
    if (!marked.has(i) || !line.startsWith("--- ") || !line.endsWith(" ---") || line.length <= 8) {
      return line;
    }
    const name = section(line.slice(4, -4));
    return name ? `--- ${name} ---` : line;
  });
  if (r.extra.length > 0) {
    lines.push(`--- ${t("further details")} ---`, ...r.extra);
  }
  return lines.join("\n");
}

/// Only these can be answered here; questions and plans stay where the agent runs.
export function answerable(r: Request): boolean {
  return r.state === "held" && r.questions.length === 0 && r.tool_name !== "ExitPlanMode";
}

/// One line of the history, as the Mac app words it.
export function historyText(e: HistoryEvent): string {
  switch (e.event) {
    case "session_started":
      return t("Session started");
    case "prompt_received":
      return t("Task received");
    case "activity": {
      const { event: _, ...activity } = e;
      return activityText(activity as Activity);
    }
    case "permission_requested":
      return t("Approval requested");
    case "compacting":
      return t("Compacting context");
    case "turn_ended":
      return t("Turn ended");
    case "turn_failed":
      return t("Turn ended with an error");
    case "session_ended":
      return t("Session ended");
    case "allowed":
      return t("Approval allowed");
    case "denied":
      return t("Approval denied");
    case "answered":
      return t("Question answered");
  }
}

/// "5 min" since a moment, as short as the history needs it.
export function relativeShort(thenMs: number, nowMs: number): string {
  const seconds = nowMs > thenMs ? Math.floor((nowMs - thenMs) / 1000) : 0;
  if (seconds < 60) return t("{n} s", { n: seconds });
  if (seconds < 3600) return t("{n} min", { n: Math.floor(seconds / 60) });
  if (seconds < 86400) return t("{n} h", { n: Math.floor(seconds / 3600) });
  return t("{n} d", { n: Math.floor(seconds / 86400) });
}

/// "128K", "1M", "1.5M".
export function tokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return t("{n}K", { n: Math.floor(n / 1000) });
  const m = n / 1_000_000;
  if (m === Math.round(m)) return t("{n}M", { n: m });
  const [whole, tenth] = m.toFixed(1).split(".");
  return t("{whole}.{tenth}M", { whole, tenth: tenth ?? "0" });
}

export function contextText(a: Agent): string | null {
  if (!a.context || a.context.window_tokens === 0) return null;
  return t("{used} of {window}", {
    used: tokens(a.context.used_tokens),
    window: tokens(a.context.window_tokens),
  });
}

/// How full the context is, 0 to 1, or null when unknown.
export function contextFill(a: Agent): number | null {
  if (!a.context || a.context.window_tokens === 0) return null;
  return Math.min(1, a.context.used_tokens / a.context.window_tokens);
}

/// "$1.23" from millionths of a US dollar; under a cent "< $0.01".
export function costAmount(microUsd: number): string {
  if (microUsd === 0) return t("$0.00");
  if (microUsd < 10_000) return t("< $0.01");
  const cents = Math.floor(microUsd / 10_000) + (microUsd % 10_000 >= 5_000 ? 1 : 0);
  const rest = cents % 100;
  return t("${dollars}.{cents}", {
    dollars: Math.floor(cents / 100),
    cents: (rest < 10 ? "0" : "") + rest,
  });
}

/// The status filter of the main window.
export type Filter = "all" | "waiting" | "working" | "done" | "other";
export const filters: Filter[] = ["all", "waiting", "working", "done", "other"];

export function filterTitle(f: Filter): string {
  switch (f) {
    case "all":
      return t("All");
    case "waiting":
      return t("Waiting");
    case "working":
      return t("Working");
    case "done":
      return t("Done");
    case "other":
      return t("Other");
  }
}

export function matches(f: Filter, s: Status): boolean {
  switch (f) {
    case "all":
      return true;
    case "waiting":
      return s === "blocked";
    case "working":
      return s === "working";
    case "done":
      return s === "done";
    case "other":
      return s === "idle" || s === "error" || s === "exited";
  }
}
