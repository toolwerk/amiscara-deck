<script lang="ts">
  import { type Agent, detailLine, displayName, statusText } from "./deck";

  let { agent: a }: { agent: Agent } = $props();
</script>

<div class="line">
  <span class="dot {a.status}"></span>
  <span class="name">{displayName(a)}</span>
  <span class="status">{statusText(a.status)}</span>
</div>
{#if detailLine(a)}
  <div class="detail">{detailLine(a)}</div>
{/if}

<style>
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
  .detail {
    color: var(--muted);
    font-size: 12px;
  }
  .detail {
    margin-top: 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
