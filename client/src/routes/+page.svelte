<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { chat } from '$lib/stores/chat';
	import Header from '$lib/components/Header.svelte';
	import ChatMessage from '$lib/components/ChatMessage.svelte';
	import ChatInput from '$lib/components/ChatInput.svelte';
	import PipelineEditor from '$lib/components/PipelineEditor.svelte';
	import type { PipelineInfo } from '$lib/types';

	const { messages, isConnected, isStreaming, isThinking, models, selectedModel, templates, pipelines, selectedPipeline, pipelineConfig, pipelineModified, modelStatus } = chat;
	const WS_URL = 'ws://localhost:8000/ws';

	let inputText = '';
	let messagesContainer: HTMLDivElement;
	let prevModel = '';
	let showEditor = false;

	onMount(() => {
		chat.connect(WS_URL);
		return () => chat.disconnect();
	});

	$: if ($selectedModel !== prevModel) {
		handleModelChange(prevModel, $selectedModel);
		prevModel = $selectedModel;
	}

	function handleModelChange(prev: string, next: string) {
		const prevIsLocal = prev && chat.isLocalModel(prev);
		const nextIsLocal = next && chat.isLocalModel(next);

		// Unload GPU: switching to "none"
		if (next === 'none' && prevIsLocal) {
			chat.unload(prev);
			return;
		}

		// Switching to a local model: wake it (and unload previous if also local)
		if (nextIsLocal) {
			const prevToUnload = prevIsLocal ? prev : undefined;
			chat.wake(next, prevToUnload);
			return;
		}

		// Switching from local to cloud: unload the local model
		if (prevIsLocal && !nextIsLocal) {
			chat.unload(prev);
		}
	}

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	$: if ($messages || $isThinking) {
		scrollToBottom();
	}

	function handleSend() {
		if (!inputText.trim() || $isStreaming) return;
		chat.send(inputText);
		inputText = '';
	}

	$: if ($selectedPipeline === '__new__') {
		createNewPipeline();
	}

	function createNewPipeline() {
		const id = `custom_${Date.now()}`;
		const blank: PipelineInfo = {
			id,
			name: 'New Agent',
			description: '',
			nodes: [
				{ id: 'llm1', node_type: 'llm', model: null, prompt: 'You are a helpful assistant.' }
			],
			edges: [
				{ from: 'input', to: 'llm1' },
				{ from: 'llm1', to: 'output' }
			]
		};
		chat.pipelineConfig.set(blank);
		chat.pipelineModified.set(true);
		showEditor = true;
	}

	function openEditor() {
		showEditor = true;
	}

	function closeEditor() {
		showEditor = false;
	}

	function handlePipelineUpdate(config: PipelineInfo) {
		chat.pipelineConfig.set(config);
		chat.pipelineModified.set(true);
	}

	async function handleSavePipeline(config: PipelineInfo) {
		await chat.savePipeline(config);
		showEditor = false;
	}

	function handleDeletePipeline(id: string) {
		chat.deletePipeline(id);
	}
</script>

<div class="app">
	<Header
		isConnected={$isConnected}
		models={$models}
		bind:selectedModel={$selectedModel}
		pipelines={$pipelines}
		bind:selectedPipeline={$selectedPipeline}
		modelStatus={$modelStatus}
		pipelineModified={$pipelineModified}
		onEditPipeline={openEditor}
		onDeletePipeline={handleDeletePipeline}
	/>

	<main>
		<div class="messages" bind:this={messagesContainer}>
			{#each $messages as message}
				<ChatMessage
					user={message.user}
					msg={message.msg}
					streaming={message.streaming}
					metadata={message.metadata}
				/>
			{/each}
			{#if $isThinking}
				<div class="message bot thinking">
					<span class="thinking-dots">
						<span></span>
						<span></span>
						<span></span>
					</span>
				</div>
			{/if}
		</div>

		<ChatInput
			bind:value={inputText}
			disabled={!$isConnected || $selectedModel === 'none'}
			sendDisabled={!$isConnected || $isStreaming || !inputText.trim() || $selectedModel === 'none'}
			onSend={handleSend}
		/>
	</main>
</div>

{#if showEditor}
	{#if $pipelineConfig}
		<PipelineEditor
			config={$pipelineConfig}
			models={$models}
			templates={$templates}
			onUpdate={handlePipelineUpdate}
			onSave={handleSavePipeline}
		/>
	{:else}
		<div style="position:fixed;inset:0;z-index:1000;background:#1a1a1a;display:flex;align-items:center;justify-content:center;color:white;">
			<div>
				<p>No pipeline config loaded</p>
				<button onclick={closeEditor}>Close</button>
			</div>
		</div>
	{/if}
{/if}
