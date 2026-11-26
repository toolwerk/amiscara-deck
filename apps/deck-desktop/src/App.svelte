<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "./i18n";
  import AgentLine from "./AgentLine.svelte";
  import RequestCard from "./RequestCard.svelte";
  import { deck, follow, openMain } from "./store.svelte";

  onMount(follow);

  const waiting = $derived(
    deck.view?.agents.filter((a) => a.request?.state === "held").length ?? 0,
  );
</script>

<main>
  <header>
    <strong>Amiscara Deck</strong>
    {#if deck.view?.connected}
      <span class="count" class:hot={waiting > 0}>
        {waiting > 0 ? t("{n} waiting", { n: waiting }) : t("Nobody is waiting.")}
      </span>
    {/if}
  </header>

  {#if deck.view === null}
    <p class="note">{t("Connecting to the daemon …")}</p>
  {:else if !deck.view.connected}
    <p class="note warn">{deck.view.problem}</p>
  {:else if deck.view.agents.length === 0}
    <p class="note">{t("No agent yet. Start Claude Code in a terminal.")}</p>
  {:else}
    <ul>
      {#each deck.view.agents as a (a.id)}
        <li class="agent" class:held={a.request?.state === "held"}>
          <AgentLine agent={a} />
          {#if a.request}
            <RequestCard request={a.request} />
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if deck.error}
    <p class="note warn">{deck.error}</p>
  {/if}
  <footer>
    <button class="link" onclick={openMain}>{t("Open main window")}</button>
  </footer>
</main>

<style>
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
  footer {
    margin-top: 12px;
    text-align: center;
  }
  .link {
    border: none;
    background: none;
    color: var(--accent);
  }
</style>
