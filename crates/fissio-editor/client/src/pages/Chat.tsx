import { onMount, createSignal, createEffect, For, Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { chat } from '../lib/stores/chat';
import Header from '../lib/components/Header';
import ChatMessage from '../lib/components/ChatMessage';
import ChatInput from '../lib/components/ChatInput';
import type { PipelineInfo } from '../lib/types';

export default function Chat() {
  const navigate = useNavigate();
  const [inputText, setInputText] = createSignal('');
  const [prevModel, setPrevModel] = createSignal('');
  const [prevPipeline, setPrevPipeline] = createSignal('');
  let messagesContainer: HTMLDivElement | undefined;

  onMount(() => {
    chat.connect();
  });

  const handleModelChange = (prev: string, next: string) => {
    const prevIsLocal = prev && chat.isLocalModel(prev);
    const nextIsLocal = next && chat.isLocalModel(next);

    if (next === 'none' && prevIsLocal) {
      chat.unload(prev);
      return;
    }

    if (nextIsLocal) {
      const prevToUnload = prevIsLocal ? prev : undefined;
      chat.wake(next, prevToUnload);
      return;
    }

    if (prevIsLocal && !nextIsLocal) {
      chat.unload(prev);
    }
  };

  createEffect(() => {
    const current = chat.selectedModel();
    const prev = prevModel();
    if (current !== prev) {
      handleModelChange(prev, current);
      setPrevModel(current);
    }
  });

  const scrollToBottom = () => {
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  };

  createEffect(() => {
    chat.messages();
    chat.isThinking();
    scrollToBottom();
  });

  createEffect(() => {
    const current = chat.selectedPipeline();
    const prev = prevPipeline();
    if (current === '__new__' && prev !== '__new__') {
      createNewPipeline();
      setPrevPipeline('__new__');
      return;
    }
    if (current === '__new__') return;
    setPrevPipeline(current);
  });

  const handleSend = () => {
    if (!inputText().trim() || chat.isStreaming()) return;

    const trimmed = inputText().trim().toLowerCase();

    if (trimmed === '/compose') {
      chat.enterComposeMode();
      setInputText('');
      return;
    }

    if (trimmed === '/done' && chat.composeMode() === 'composing') {
      chat.send('/done');
      setInputText('');
      return;
    }

    chat.send(inputText());
    setInputText('');
  };

  const createNewPipeline = () => {
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
    chat.setSelectedPipeline('');
    chat.setPipelineConfig(blank);
    navigate('/composer');
  };

  const openEditor = () => navigate('/composer');
  const handleDeletePipeline = (id: string) => chat.deletePipeline(id);

  const handleSaveComposed = async () => {
    const draft = chat.composeDraft();
    if (!draft) return;

    const config: PipelineInfo = {
      id: draft.id || `composed_${Date.now()}`,
      name: draft.name || 'Composed Pipeline',
      description: draft.description || '',
      nodes: (draft.nodes || [])
        .filter((n): n is typeof n & { id: string; node_type: string } => !!n.id && !!n.node_type)
        .map((n) => ({
          id: n.id,
          node_type: n.node_type,
          model: n.model ?? null,
          prompt: n.prompt ?? null
        })),
      edges: draft.edges || []
    };

    await chat.savePipeline(config);
    chat.setSelectedPipeline(config.id);
    chat.exitComposeMode();
  };

  const handleCancelCompose = () => chat.exitComposeMode();

  return (
    <div class="app">
      <Header
        isConnected={chat.isConnected()}
        models={chat.models()}
        selectedModel={chat.selectedModel()}
        onModelChange={chat.setSelectedModel}
        pipelines={chat.pipelines()}
        selectedPipeline={chat.selectedPipeline()}
        onPipelineChange={chat.setSelectedPipeline}
        modelStatus={chat.modelStatus()}
        pipelineModified={chat.pipelineModified()}
        onEditPipeline={openEditor}
        onDeletePipeline={handleDeletePipeline}
      />

      <main>
        <div class="messages" ref={messagesContainer}>
          <For each={chat.messages()}>
            {(message) => (
              <ChatMessage
                user={message.user}
                msg={message.msg}
                streaming={message.streaming}
                metadata={message.metadata}
              />
            )}
          </For>
          <Show when={chat.isThinking()}>
            <div class="message bot thinking">
              <span class="thinking-dots">
                <span />
                <span />
                <span />
              </span>
            </div>
          </Show>
        </div>

        <Show when={chat.composeMode() === 'composing'}>
          <div class="compose-indicator">
            <span class="compose-badge">COMPOSE MODE</span>
            <span class="compose-hint">Type <code>/done</code> when design is complete</span>
          </div>
        </Show>

        <Show when={chat.composeMode() === 'finalizing' && chat.composeDraft()}>
          <div class="compose-preview">
            <div class="compose-preview-header">
              <h4>{chat.composeDraft()?.name || 'Unnamed Pipeline'}</h4>
              <span class="compose-preview-meta">
                {chat.composeDraft()?.nodes?.length || 0} nodes, {chat.composeDraft()?.edges?.length || 0} edges
              </span>
            </div>
            <p class="compose-preview-desc">{chat.composeDraft()?.description || 'No description'}</p>
            <div class="compose-preview-actions">
              <button class="btn-save" onClick={handleSaveComposed}>Save & Use</button>
              <button class="btn-cancel" onClick={handleCancelCompose}>Cancel</button>
            </div>
          </div>
        </Show>

        <ChatInput
          value={inputText()}
          onValueChange={setInputText}
          disabled={!chat.isConnected() || chat.selectedModel() === 'none'}
          sendDisabled={!chat.isConnected() || chat.isStreaming() || !inputText().trim() || chat.selectedModel() === 'none'}
          onSend={handleSend}
        />
      </main>
    </div>
  );
}
