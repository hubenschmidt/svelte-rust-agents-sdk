<script lang="ts">
	import { goto } from '$app/navigation';
	import { chat } from '$lib/stores/chat';
	import PipelineEditor from '$lib/components/PipelineEditor.svelte';
	import type { PipelineInfo } from '$lib/types';

	const { pipelineConfig, models, templates, availableTools } = chat;

	function handlePipelineUpdate(config: PipelineInfo) {
		chat.pipelineConfig.set(config);
		chat.pipelineModified.set(true);
	}

	async function handleSavePipeline(config: PipelineInfo) {
		await chat.savePipeline(config);
		chat.selectedPipeline.set(config.id);
		goto('/');
	}
</script>

{#if $pipelineConfig}
	<PipelineEditor
		config={$pipelineConfig}
		models={$models}
		templates={$templates}
		availableTools={$availableTools}
		onUpdate={handlePipelineUpdate}
		onSave={handleSavePipeline}
	/>
{:else}
	<div class="no-config">
		<p>No agent config loaded</p>
		<button onclick={() => goto('/')}>Back to Chat</button>
	</div>
{/if}

<style>
	.no-config {
		position: fixed;
		inset: 0;
		background: #1a1a1a;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		color: white;
		gap: 1rem;
	}

	.no-config button {
		padding: 0.5rem 1rem;
		background: var(--accent, #4f46e5);
		color: white;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}

	.no-config button:hover {
		background: var(--accent-hover, #6366f1);
	}
</style>
