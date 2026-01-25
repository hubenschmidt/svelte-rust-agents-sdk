<script lang="ts">
	import Settings from './Settings.svelte';
	import type { ModelConfig, PipelineInfo } from '$lib/types';

	export let isConnected: boolean;
	export let models: ModelConfig[];
	export let selectedModel: string;
	export let pipelines: PipelineInfo[];
	export let selectedPipeline: string;
	export let modelStatus: string;
	export let pipelineModified: boolean = false;
	export let onEditPipeline: () => void = () => {};
	export let onDeletePipeline: (id: string) => void = () => {};

	$: statusText = modelStatus === 'loading' ? 'Loading model...' :
		modelStatus === 'unloading' ? 'Unloading model...' : '';

	$: currentPipeline = pipelines.find(p => p.id === selectedPipeline);
</script>

<div class="header-container">
	<header>
		<div class="status" class:connected={isConnected}></div>
		<b>agents-rs</b>
		{#if statusText}
			<span class="model-status">{statusText}</span>
		{/if}
		<div class="selectors" class:no-status={!statusText}>
			<div class="pipeline-wrapper">
				<select bind:value={selectedPipeline} class="pipeline-select" disabled={!isConnected || !!statusText} title="Agent Config">
					{#each pipelines as pipeline}
						<option value={pipeline.id} title={pipeline.description}>{pipeline.name}</option>
					{/each}
					<option value="__new__">-- Define agent... --</option>
				</select>
				{#if currentPipeline}
					<button
						class="delete-btn"
						onclick={() => onDeletePipeline(selectedPipeline)}
						title="Delete config"
					>✕</button>
				{/if}
				<button
					class="edit-btn"
					class:modified={pipelineModified}
					onclick={onEditPipeline}
					disabled={!isConnected || !currentPipeline}
					title="Edit pipeline"
				>
					✎
				</button>
			</div>
			<select bind:value={selectedModel} class="model-select" disabled={!isConnected || !!statusText} title="Default Model">
				<option value="none">-- Unload GPU --</option>
				{#each models as model}
					<option value={model.id}>{model.name}</option>
				{/each}
			</select>
		</div>
		<Settings />
	</header>
</div>

<style>
	.header-container {
		display: flex;
		flex-direction: column;
	}

	.model-status {
		margin-left: auto;
		font-size: 0.875rem;
		color: var(--text-secondary, #888);
		font-style: italic;
	}

	.selectors {
		display: flex;
		gap: 0.5rem;
		margin-left: 0.5rem;
	}

	.selectors.no-status {
		margin-left: auto;
	}

	.pipeline-wrapper {
		display: flex;
		gap: 0;
	}

	.pipeline-select,
	.model-select {
		padding: 0.25rem 0.5rem;
		border-radius: 4px;
		border: 1px solid var(--border);
		background: var(--bg-secondary);
		color: var(--text);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.pipeline-select {
		border-top-right-radius: 0;
		border-bottom-right-radius: 0;
		border-right: none;
	}

	.edit-btn {
		padding: 0.25rem 0.5rem;
		border-radius: 0 4px 4px 0;
		border: 1px solid var(--border);
		background: var(--bg-secondary);
		color: var(--text);
		font-size: 0.875rem;
		cursor: pointer;
	}

	.edit-btn:hover:not(:disabled) {
		background: var(--accent, #3b82f6);
	}

	.edit-btn.modified {
		border-color: #f59e0b;
		color: #f59e0b;
	}

	.edit-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.delete-btn {
		padding: 0.25rem 0.4rem;
		border: 1px solid var(--border);
		border-left: none;
		border-radius: 0;
		background: var(--bg-secondary);
		color: #ef4444;
		font-size: 0.75rem;
		cursor: pointer;
	}

	.delete-btn:hover {
		background: #ef4444;
		color: white;
	}

	.pipeline-select:disabled,
	.model-select:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.status {
		width: 10px;
		height: 10px;
		border-radius: 50%;
		background: #ef4444;
		margin-right: 0.5rem;
		flex-shrink: 0;
	}

	.status.connected {
		background: #22c55e;
	}
</style>
