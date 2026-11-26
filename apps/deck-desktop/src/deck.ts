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
  request: Request | null;
}

export interface View {
  connected: boolean;
  problem: string | null;
  agents: Agent[];
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
