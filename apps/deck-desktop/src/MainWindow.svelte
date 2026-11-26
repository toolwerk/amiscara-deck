<script lang="ts">
  // The main window: agents in a grid with a status filter, and a column with
  // the details of the selected one (request, last task, key figures, history).
  import { onMount } from "svelte";
  import { t } from "./i18n";
  import AgentLine from "./AgentLine.svelte";
  import RequestCard from "./RequestCard.svelte";
  import {
    type Filter,
    contextFill,
    contextText,
    costAmount,
    displayName,
    filterTitle,
    filters,
    historyText,
    matches,
    relativeShort,
    statusText,
  } from "./deck";
  import { deck, follow } from "./store.svelte";

  let filter = $state<Filter>("all");
  let selection = $state<string | null>(null);
  let now = $state(Date.now());

  onMount(() => {
    const stop = follow();
    const clock = setInterval(() => (now = Date.now()), 10_000);
    return () => {
      stop();
      clearInterval(clock);
    };
  });

  const all = $derived(deck.view?.agents ?? []);
  const shown = $derived(all.filter((a) => matches(filter, a.status)));
  // The selection stays on its agent while the list reorders; only when it
  // leaves the list does the first one take its place.
  $effect(() => {
    if (!shown.some((a) => a.id === selection)) selection = shown[0]?.id ?? null;
  });
  const selected = $derived(shown.find((a) => a.id === selection) ?? null);
  const today = $derived(deck.view?.today_micro_usd ?? null);
</script>

<div class="window">
  <section class="list">
    <nav>
      {#each filters as f}
        <button class="chip" class:on={filter === f} aria-pressed={filter === f} onclick={() => (filter = f)}>
          {filterTitle(f)} {all.filter((a) => matches(f, a.status)).length}
        </button>
      {/each}
      <span class="spacer"></span>
      {#if today !== null && today > 0}
        <span class="cost" title={t("Estimate of Claude Code, not a billed amount.")}>
          {t("Today ≈ {amount}", { amount: costAmount(today) })}
        </span>
      {/if}
    </nav>
    {#if deck.view === null}
      <p class="empty note">{t("Connecting to the daemon …")}</p>
    {:else if !deck.view.connected}
      <p class="empty note warn">{deck.view.problem}</p>
    {:else if shown.length === 0}
      <p class="empty note">
        {all.length === 0
          ? t("No agent yet. Start Claude Code in a terminal.")
          : t("No agent in this selection.")}
      </p>
    {:else}
      <div class="grid">
        {#each shown as a (a.id)}
          <button
            class="card"
            class:selected={a.id === selected?.id}
            class:held={a.request?.state === "held"}
            onclick={() => (selection = a.id)}
          >
            <AgentLine agent={a} />
          </button>
        {/each}
      </div>
    {/if}
    {#if deck.error}
      <p class="note warn pad">{deck.error}</p>
    {/if}
  </section>

  {#if selected}
    {@const a = selected}
    <aside>
      <div class="head">
        <span class="dot {a.status}"></span>
        <strong>{displayName(a)}</strong>
        <span class="spacer"></span>
        <span class="muted">{statusText(a.status)}</span>
      </div>
      {#if a.branch}
        <div class="muted">{a.branch}</div>
      {/if}
      {#if a.request}
        <RequestCard request={a.request} tall />
      {/if}
      {#if a.last_prompt}
        <h3>{t("Last task")}</h3>
        <p class="prompt">{a.last_prompt}</p>
      {/if}
      <h3>{t("Key figures")}</h3>
      <dl>
        <dt>{t("Context")}</dt>
        <dd>{contextText(a) ?? t("unknown")}</dd>
        <dt>{t("Turns")}</dt>
        <dd>{a.turns ?? t("unknown")}</dd>
        <dt>{t("Subagents")}</dt>
        <dd>{a.subagents}</dd>
        <dt>{t("Cost (estimate)")}</dt>
        <dd>{a.cost_micro_usd !== null ? costAmount(a.cost_micro_usd) : t("unknown")}</dd>
      </dl>
      {#if contextFill(a) !== null}
        <div class="bar"><div style="width: {(contextFill(a) ?? 0) * 100}%"></div></div>
      {/if}
      <h3>{t("History")}</h3>
      {#if a.history.length === 0}
        <p class="muted">{t("Nothing recorded yet.")}</p>
      {:else}
        <ol>
          {#each [...a.history].reverse() as entry}
            <li>
              <span class="when">{relativeShort(entry.at_ms, now)}</span>
              <span>{historyText(entry.event)}</span>
            </li>
          {/each}
        </ol>
      {/if}
    </aside>
  {/if}
</div>

<style>
  .window {
    display: flex;
    height: 100vh;
  }
  .list {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  nav {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 10px 16px;
    border-bottom: 1px solid var(--line);
  }
  .chip {
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 12px;
  }
  .chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
  .cost,
  .muted {
    color: var(--muted);
    font-size: 12px;
  }
  .empty {
    margin: auto;
  }
  .pad {
    padding: 0 16px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 12px;
    padding: 16px;
    overflow: auto;
    align-content: start;
  }
  .card {
    text-align: left;
    background: var(--card);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 8px 10px;
    min-width: 0;
  }
  .card.held {
    border-color: var(--wait);
  }
  .card.selected {
    outline: 2px solid var(--accent);
    outline-offset: -1px;
  }
  aside {
    width: 340px;
    flex: none;
    overflow: auto;
    padding: 16px;
    background: var(--card);
    border-left: 1px solid var(--line);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
  }
  h3 {
    margin: 16px 0 6px;
    font-size: 11px;
    color: var(--muted);
  }
  .prompt {
    margin: 0;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 10em;
    overflow: auto;
    user-select: text;
  }
  dl {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 6px;
    margin: 0;
    font-size: 12px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    font-weight: 500;
  }
  .bar {
    margin-top: 8px;
    height: 4px;
    background: var(--line);
    border-radius: 2px;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 12px;
  }
  .when {
    display: inline-block;
    width: 48px;
    text-align: right;
    margin-right: 8px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
</style>
