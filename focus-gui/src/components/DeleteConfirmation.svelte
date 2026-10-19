<script lang="ts">
  import { onMount } from "svelte";
  let { name, consequences, onCancel, onDelete }: {
    name: string;
    consequences: string;
    onCancel: () => void;
    onDelete: () => void;
  } = $props();
  let cancelButton: HTMLButtonElement;
  let deleteButton: HTMLButtonElement;
  onMount(() => { cancelButton.focus(); });
  function keydown(event: KeyboardEvent): void {
    if (event.key === "Escape") onCancel();
    if (event.key === "Tab") {
      event.preventDefault();
      (document.activeElement === cancelButton ? deleteButton : cancelButton).focus();
    }
  }
</script>

<div class="onboarding-backdrop" role="presentation">
  <div class="onboarding-modal" role="dialog" aria-modal="true" aria-labelledby="delete-title" tabindex="-1" onkeydown={keydown}>
    <h2 id="delete-title">Delete “{name}”?</h2>
    <p>{consequences} This cannot be undone.</p>
    <div class="button-row onboarding-actions">
      <button bind:this={cancelButton} class="secondary" onclick={onCancel}>Cancel</button>
      <button bind:this={deleteButton} class="secondary danger-action" onclick={onDelete}>Delete</button>
    </div>
  </div>
</div>
