<script lang="ts">
  import { t } from "./i18n";
  import { type Request, answerable, fullText, requestTitle } from "./deck";
  import { answer, deck } from "./store.svelte";

  let { request: r, tall = false }: { request: Request; tall?: boolean } = $props();

  /// "Allow" waits a moment after a request appears or opens in full: a click
  /// meant for "View" or for a card that just moved must not allow another one.
  const ARM_MS = 800;
  let armed = $state<string | null>(null);
  const key = $derived(`${r.id}:${deck.expanded[r.id] ? 1 : 0}`);
  $effect(() => {
    const k = key;
    armed = null;
    const timer = setTimeout(() => (armed = k), ARM_MS);
    return () => clearTimeout(timer);
  });
</script>

<div class="request">
  <div class="title">{requestTitle(r.tool_name)}</div>
  <pre class:tall>{deck.expanded[r.id] || !r.truncated ? fullText(r) : r.summary}</pre>
  {#if answerable(r)}
    <div class="buttons">
      {#if r.truncated && !deck.expanded[r.id]}
        <button onclick={() => (deck.expanded[r.id] = true)}>{t("View")}</button>
      {:else}
        <button class="allow" disabled={armed !== key} onclick={() => answer(r.id, true)}>{t("Allow")}</button>
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

<style>
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
  pre.tall {
    max-height: 420px;
  }
  .buttons {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
  }
  .hint {
    color: var(--muted);
    font-size: 12px;
  }
</style>
