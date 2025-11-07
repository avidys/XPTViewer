<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/api/dialog';
  import { onMount } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import type { VirtualItem, Virtualizer } from '@tanstack/virtual-core';

  type Field = {
    name: string;
    label?: string;
    type: string;
  };

  type DatasetRow = Record<string, unknown>;

  type Dataset = {
    name: string;
    label?: string;
    observationCount: number;
    fields: Field[];
    rows: DatasetRow[];
  };

  type XptFile = {
    path: string;
    datasets: Dataset[];
  };

  let fileData: XptFile | null = null;
  let errorMessage: string | null = null;
  let selectedDatasetIndex = 0;
  let mounted = false;
  let tableContainer: HTMLDivElement | null = null;
  let rowVirtualizer: Virtualizer<HTMLDivElement, Element> | null = null;
  let datasetKey: string | null = null;
  let currentDatasetKey: string | null = null;
  let virtualRows: VirtualItem[] = [];
  let totalSize = 0;
  let gridTemplate = '';
  let rowCount = 0;

  const handleOpenFile = async () => {
    errorMessage = null;

    try {
      const selected = await open({
        filters: [{ name: 'SAS Transport', extensions: ['xpt', 'xport'] }]
      });

      if (!selected || Array.isArray(selected)) {
        return;
      }

      const data = await invoke<XptFile>('load_xpt', { path: selected });
      fileData = data;
      selectedDatasetIndex = 0;
    } catch (error) {
      console.error(error);
      errorMessage =
        error instanceof Error ? error.message : 'Unable to open the selected XPT file.';
    }
  };

  onMount(() => {
    mounted = true;
  });

  $: selectedDataset = fileData?.datasets[selectedDatasetIndex];
  $: rowCount = selectedDataset?.rows.length ?? 0;
  $: gridTemplate =
    selectedDataset && selectedDataset.fields.length > 0
      ? `repeat(${selectedDataset.fields.length}, minmax(140px, 1fr))`
      : 'minmax(140px, 1fr)';
  $: virtualRows = rowVirtualizer ? rowVirtualizer.getVirtualItems() : [];
  $: totalSize = rowVirtualizer ? rowVirtualizer.getTotalSize() : 0;
  $: currentDatasetKey = selectedDataset
    ? `${fileData?.path ?? ''}:${selectedDatasetIndex}`
    : null;

  $: if (!rowVirtualizer && tableContainer) {
    rowVirtualizer = createVirtualizer({
      count: rowCount,
      getScrollElement: () => tableContainer as HTMLDivElement,
      estimateSize: () => 44,
      overscan: 12
    });
  }

  $: if (rowVirtualizer) {
    rowVirtualizer.setOptions((prev) => ({
      ...prev,
      count: rowCount
    }));
  }

  $: if (rowVirtualizer && datasetKey !== currentDatasetKey) {
    datasetKey = currentDatasetKey;
    rowVirtualizer.scrollToOffset(0);
  }

  function formatValue(value: unknown): string {
    if (value === null || value === undefined) {
      return '—';
    }

    if (typeof value === 'number') {
      if (Number.isInteger(value)) {
        return value.toString();
      }
      return value.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
    }

    if (value instanceof Date) {
      return value.toISOString();
    }

    if (typeof value === 'object') {
      return JSON.stringify(value);
    }

    if (typeof value === 'string') {
      const trimmed = value.trim();
      return trimmed.length > 0 ? trimmed : '—';
    }

    return String(value);
  }
</script>

<main class="app-shell">
  <section class="panel">
    <header>
      <h1>XPTViewer</h1>
      <p>A modern desktop viewer for SAS XPORT (XPT) datasets.</p>
      <button class="primary" on:click={handleOpenFile} disabled={!mounted}>
        Open XPT File
      </button>
      {#if fileData}
        <p class="path">{fileData.path}</p>
      {/if}
    </header>

    {#if errorMessage}
      <div class="banner error">{errorMessage}</div>
    {/if}

    {#if fileData}
      <div class="dataset-summary">
        <h2>Datasets ({fileData.datasets.length})</h2>
        <div class="dataset-list">
          {#each fileData.datasets as dataset, index}
            <button
              class:selected={selectedDatasetIndex === index}
              on:click={() => (selectedDatasetIndex = index)}
            >
              <span class="dataset-name">{dataset.name}</span>
              {#if dataset.label}
                <span class="dataset-label">{dataset.label}</span>
              {/if}
              <span class="dataset-meta">{dataset.observationCount} rows</span>
            </button>
          {/each}
        </div>
      </div>
    {:else}
      <div class="empty-state">
        <p>
          Select a SAS XPT file to inspect metadata, schema, and the first rows of each dataset
          it contains.
        </p>
      </div>
    {/if}
  </section>

  {#if selectedDataset}
    <section class="panel details">
      <header>
        <h2>{selectedDataset.name}</h2>
        {#if selectedDataset.label}
          <p class="dataset-label">{selectedDataset.label}</p>
        {/if}
        <p class="dataset-meta">{selectedDataset.observationCount} total observations</p>
      </header>

      <div class="schema">
        <h3>Variables</h3>
        <table>
          <thead>
            <tr>
              <th>Name</th>
              <th>Label</th>
              <th>Type</th>
            </tr>
          </thead>
          <tbody>
            {#each selectedDataset.fields as field}
              <tr>
                <td>{field.name}</td>
                <td>{field.label ?? '—'}</td>
                <td class="type">{field.type}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>

      <div class="data-preview">
        <h3>Preview</h3>
        <p class="hint">
          Showing {selectedDataset.rows.length.toLocaleString()} row{selectedDataset.rows.length === 1
            ? ''
            : 's'}.
        </p>
        <div class="data-grid">
          <div class="grid-row header" style={`grid-template-columns: ${gridTemplate};`} role="row">
            {#each selectedDataset.fields as field}
              <div class="grid-cell header-cell" role="columnheader">{field.name}</div>
            {/each}
          </div>
          <div
            class="grid-body"
            bind:this={tableContainer}
            role="presentation"
            aria-rowcount={selectedDataset.rows.length}
          >
            <div class="grid-spacer" style={`height: ${totalSize}px;`}>
              {#if rowVirtualizer}
                {#each virtualRows as virtualRow (virtualRow.key)}
                  {@const row = selectedDataset.rows[virtualRow.index]}
                  <div
                    class="grid-row virtual-row"
                    class:odd={virtualRow.index % 2 === 1}
                    style={`grid-template-columns: ${gridTemplate}; transform: translateY(${virtualRow.start}px);`}
                    role="row"
                    aria-rowindex={virtualRow.index + 1}
                  >
                    {#each selectedDataset.fields as field}
                      <div class="grid-cell" role="gridcell">{formatValue(row[field.name])}</div>
                    {/each}
                  </div>
                {/each}
              {/if}
            </div>
          </div>
        </div>
      </div>
    </section>
  {/if}
</main>

<style>
  main.app-shell {
    display: grid;
    grid-template-columns: minmax(280px, 360px) 1fr;
    gap: 1.5rem;
    max-width: 1200px;
    width: min(1200px, 96vw);
    margin: 3rem auto;
    background: rgba(255, 255, 255, 0.9);
    border-radius: 1.5rem;
    box-shadow: 0 25px 65px rgba(15, 23, 42, 0.25);
    padding: 1.75rem;
    backdrop-filter: blur(20px);
  }

  section.panel {
    background: rgba(248, 250, 252, 0.9);
    border-radius: 1.25rem;
    padding: 1.5rem;
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6);
  }

  section.panel header h1,
  section.panel header h2 {
    margin: 0;
    font-weight: 700;
    color: #1f2937;
  }

  section.panel header p {
    margin: 0.25rem 0 0;
    color: #475569;
  }

  button.primary {
    margin-top: 1rem;
    align-self: flex-start;
    background: linear-gradient(135deg, #2563eb, #4f46e5);
    color: white;
    border: none;
    border-radius: 999px;
    padding: 0.75rem 1.5rem;
    font-size: 0.95rem;
    font-weight: 600;
    cursor: pointer;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  button.primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  button.primary:not(:disabled):hover {
    transform: translateY(-1px);
    box-shadow: 0 12px 24px rgba(79, 70, 229, 0.3);
  }

  .path {
    font-size: 0.8rem;
    color: #6b7280;
    word-break: break-all;
  }

  .banner.error {
    background: rgba(248, 113, 113, 0.1);
    color: #b91c1c;
    border-radius: 0.75rem;
    padding: 0.75rem 1rem;
  }

  .dataset-summary h2 {
    margin: 0 0 0.5rem;
    font-size: 1rem;
    color: #1f2937;
  }

  .dataset-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .dataset-list button {
    border: none;
    border-radius: 0.75rem;
    padding: 0.75rem 1rem;
    text-align: left;
    background: rgba(255, 255, 255, 0.7);
    cursor: pointer;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    transition: background 0.15s ease, transform 0.15s ease;
  }

  .dataset-list button:hover {
    background: rgba(191, 219, 254, 0.6);
    transform: translateY(-1px);
  }

  .dataset-list button.selected {
    background: rgba(37, 99, 235, 0.12);
    border: 1px solid rgba(37, 99, 235, 0.3);
  }

  .dataset-name {
    font-weight: 600;
    color: #1e293b;
  }

  .dataset-label {
    font-size: 0.85rem;
    color: #475569;
  }

  .dataset-meta {
    font-size: 0.75rem;
    color: #6366f1;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .empty-state {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    color: #475569;
    padding: 2rem;
    border: 2px dashed rgba(148, 163, 184, 0.35);
    border-radius: 1rem;
    background: rgba(255, 255, 255, 0.5);
  }

  section.panel.details {
    overflow: hidden;
  }

  .schema table {
    width: 100%;
    border-collapse: collapse;
    background: white;
    border-radius: 1rem;
    overflow: hidden;
    box-shadow: 0 12px 24px rgba(15, 23, 42, 0.08);
  }

  .schema thead {
    background: linear-gradient(135deg, #2563eb, #4f46e5);
    color: white;
  }

  th,
  td {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid rgba(148, 163, 184, 0.3);
    font-size: 0.9rem;
  }

  td.type {
    text-transform: uppercase;
    font-size: 0.75rem;
    letter-spacing: 0.05em;
    color: #6366f1;
  }

  .schema tbody tr:hover {
    background: rgba(59, 130, 246, 0.08);
  }

  .data-grid {
    border-radius: 1rem;
    overflow: hidden;
    background: white;
    box-shadow: 0 12px 24px rgba(15, 23, 42, 0.08);
    border: 1px solid rgba(148, 163, 184, 0.25);
  }

  .grid-row {
    display: grid;
    gap: 0;
  }

  .grid-row.header {
    background: linear-gradient(135deg, #2563eb, #4f46e5);
    color: white;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .grid-cell {
    padding: 0.75rem 1rem;
    border-bottom: 1px solid rgba(148, 163, 184, 0.3);
    font-size: 0.9rem;
    word-break: break-word;
  }

  .grid-cell.header-cell {
    border-bottom: none;
    font-size: 0.8rem;
  }

  .grid-body {
    max-height: 360px;
    overflow: auto;
    position: relative;
  }

  .grid-spacer {
    position: relative;
  }

  .grid-row.virtual-row {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    min-height: 44px;
  }

  .grid-row.virtual-row.odd {
    background: rgba(248, 250, 252, 0.7);
  }

  .hint {
    margin-top: -0.5rem;
    font-size: 0.8rem;
    color: #475569;
  }

  @media (max-width: 960px) {
    main.app-shell {
      grid-template-columns: 1fr;
    }

    .data-preview .grid-body {
      max-height: 240px;
    }
  }
</style>
