<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { chat } from '$lib/stores/chat';
	import type { WsMetadata } from '$lib/types';

	const { messages, isConnected, isStreaming, useEvaluator } = chat;
	const WS_URL = 'ws://localhost:8000/ws';

	let inputText = '';
	let messagesContainer: HTMLDivElement;

	onMount(() => {
		chat.connect(WS_URL);
		return () => chat.disconnect();
	});

	async function scrollToBottom() {
		await tick();
		if (messagesContainer) {
			messagesContainer.scrollTop = messagesContainer.scrollHeight;
		}
	}

	$: if ($messages) {
		scrollToBottom();
	}

	function handleSend() {
		if (!inputText.trim() || $isStreaming) return;
		chat.send(inputText);
		inputText = '';
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key !== 'Enter') return;
		if (event.shiftKey) return;
		event.preventDefault();
		handleSend();
	}

	function formatMetadata(m: WsMetadata): string {
		const secs = (m.elapsed_ms / 1000).toFixed(1);
		if (m.input_tokens > 0 || m.output_tokens > 0) {
			return `${secs}s · ${m.input_tokens}/${m.output_tokens} tokens`;
		}
		return `${secs}s`;
	}
</script>

<div class="app">
	<header>
		<div class="status" class:connected={$isConnected}></div>
		<b>agents-rs</b>
		<label class="evaluator-toggle">
			<input type="checkbox" bind:checked={$useEvaluator} />
			Evaluator
		</label>
	</header>

	<main>
		<div class="messages" bind:this={messagesContainer}>
			{#each $messages as message}
				<div
					class="message"
					class:user={message.user === 'User'}
					class:bot={message.user === 'Bot'}
					class:streaming={message.streaming}
				>
					{message.msg}
					{#if message.user === 'Bot' && message.metadata && !message.streaming}
						<div class="metadata">{formatMetadata(message.metadata)}</div>
					{/if}
				</div>
			{/each}
		</div>

		<div class="input-area">
			<textarea
				bind:value={inputText}
				onkeydown={handleKeydown}
				placeholder="Type a message..."
				disabled={!$isConnected}
				rows="1"
			></textarea>
			<button
				onclick={handleSend}
				disabled={!$isConnected || $isStreaming || !inputText.trim()}
			>
				Send
			</button>
		</div>
	</main>
</div>
