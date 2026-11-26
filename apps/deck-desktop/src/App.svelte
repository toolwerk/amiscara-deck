<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { t } from "./i18n";
  import {
    type Agent,
    type View,
    answerable,
    detailLine,
    fullText,
    requestTitle,
    statusText,
  } from "./deck";

  let view = $state<View | null>(null);
  /// Requests whose full text the user opened: only those may be allowed.
  let expanded = $state<Record<string, boolean>>({});
  let error = $state<string | null>(null);

  onMount(() => {
    const stop = listen<View>("deck://state", (e) => {
      view = e.payload;
    });
    // What was sent before this page listened.
    invoke<View | null>("current").then((v) => {
      if (v && view === null) view = v;
    });
    return () => {
      stop.then((f) => f());
    };
  });

  function name(a: Agent): string {
    return a.project ?? a.title ?? a.id;
  }

  async function answer(id: string, allow: boolean) {
    try {
      await invoke("answer", { requestId: id, allow });
      error = null;
    } catch (e) {
      error = t("Error: {error}", { error: String(e) });
    }
  }

  const waiting = $derived(
    view?.agents.filter((a) => a.request?.state === "held").length ?? 0,
  );
</script>

<main>
  <header>
    <strong>Amiscara Deck</strong>
    {#if view?.connected}
      <span class="count" class:hot={waiting > 0}>
        {waiting > 0 ? t("{n} waiting", { n: waiting }) : t("Nobody is waiting.")}
      </span>
    {/if}
  </header>

  {#if view === null}
    <p class="note">{t("Connecting to the daemon …")}</p>
  {:else if !view.connected}
    <p class="note warn">{view.problem}</p>
  {:else if view.agents.length === 0}
    <p class="note">{t("No agent yet. Start Claude Code in a terminal.")}</p>
  {:else}
    <ul>
      {#each view.agents as a (a.id)}
        <li class="agent" class:held={a.request?.state === "held"}>
          <div class="line">
            <span class="dot {a.status}"></span>
            <span class="name">{name(a)}</span>
            <span class="status">{statusText(a.status)}</span>
          </div>
          {#if detailLine(a)}
            <div class="detail">{detailLine(a)}</div>
          {/if}
          {#if a.request}
            {@const r = a.request}
            <div class="request">
              <div class="title">{requestTitle(r.tool_name)}</div>
              <pre>{expanded[r.id] || !r.truncated ? fullText(r) : r.summary}</pre>
              {#if answerable(r)}
                <div class="buttons">
                  {#if r.truncated && !expanded[r.id]}
                    <button onclick={() => (expanded[r.id] = true)}>{t("View")}</button>
                  {:else}
                    <button class="allow" onclick={() => answer(r.id, true)}>{t("Allow")}</button>
                  {/if}
                  <button onclick={() => answer(r.id, false)}>{t("Deny")}</button>
                </div>
              {:else if r.state === "held" && r.tool_name === "ExitPlanMode"}
                <p class="hint">{t("Answer in the program the agent runs in. Deck does not approve plans.")}</p>
              {:else if r.state === "held"}
                <p class="hint">{t("Deck does not hold this question. Answer in the program the agent runs in.")}</p>
              {:else if r.state === "passed_through"}
                <p class="hint">{t("Deck does not hold this request. Answer in the terminal.")}</p>
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if error}
    <p class="note warn">{error}</p>
  {/if}
</main>

<style>
  :global(:root) {
    color-scheme: light dark;
    --bg: #f6faf9;
    --card: #ffffff;
    --text: #1b2b2a;
    --muted: #5d706e;
    --accent: #0e7c74;
    --wait: #ff9500;
    --line: #dbe7e4;
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --bg: #0b2a2e;
      --card: #10363a;
      --text: #e6f4f1;
      --muted: #9fbab6;
      --line: #1d4a4e;
    }
  }
  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--text);
    font: 13px/1.4 system-ui, "Segoe UI", Ubuntu, sans-serif;
  }
  main {
    padding: 12px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin-bottom: 10px;
  }
  .count {
    color: var(--muted);
  }
  .count.hot {
    color: var(--wait);
    font-weight: 600;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .agent {
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 8px 10px;
  }
  .agent.held {
    border-color: var(--wait);
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    font-weight: 600;
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status,
  .detail,
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
  .detail {
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    flex: none;
  }
  .dot.working {
    background: var(--accent);
  }
  .dot.blocked {
    background: var(--wait);
  }
  .dot.error {
    background: #d64545;
  }
  .request {
    margin-top: 8px;
  }
  .title {
    font-weight: 600;
  }
  pre {
    margin: 4px 0;
    padding: 6px 8px;
    background: var(--bg);
    border-radius: 6px;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 220px;
    overflow: auto;
    font: 12px/1.35 ui-monospace, Consolas, monospace;
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  button {
    border: 1px solid var(--line);
    background: var(--card);
    color: var(--text);
    border-radius: 6px;
    padding: 4px 12px;
    font: inherit;
    cursor: pointer;
  }
  button.allow {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .note {
    color: var(--muted);
  }
  .warn {
    color: var(--wait);
  }
</style>
