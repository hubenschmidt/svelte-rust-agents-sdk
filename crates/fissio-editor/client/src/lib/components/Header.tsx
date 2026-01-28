import { createMemo, For, Show } from 'solid-js';
import Settings from './Settings';
import type { ModelConfig, PipelineInfo } from '../types';

type Props = {
  isConnected: boolean;
  models: ModelConfig[];
  selectedModel: string;
  onModelChange: (id: string) => void;
  pipelines: PipelineInfo[];
  selectedPipeline: string;
  onPipelineChange: (id: string) => void;
  modelStatus: string;
  pipelineModified?: boolean;
  onEditPipeline?: () => void;
  onDeletePipeline?: (id: string) => void;
};

const STATUS_TEXT: Record<string, string> = {
  loading: 'Loading model...',
  unloading: 'Unloading model...'
};

export default function Header(props: Props) {
  const statusText = createMemo(() => STATUS_TEXT[props.modelStatus] ?? '');
  const currentPipeline = createMemo(() => props.pipelines.find(p => p.id === props.selectedPipeline));
  const selectorsDisabled = createMemo(() => !props.isConnected || !!statusText());

  return (
    <div class="header-container">
      <header>
        <div class="status" classList={{ connected: props.isConnected }} />
        <b>fissio</b>
        <Show when={statusText()}>
          <span class="model-status">{statusText()}</span>
        </Show>
        <div class="selectors" classList={{ 'no-status': !statusText() }}>
          <div class="pipeline-wrapper">
            <select
              class="pipeline-select"
              value={props.selectedPipeline}
              onChange={(e) => props.onPipelineChange(e.currentTarget.value)}
              disabled={selectorsDisabled()}
              title="Agent Config"
            >
              <option value="">Direct Chat</option>
              <For each={props.pipelines}>
                {(pipeline) => (
                  <option value={pipeline.id} title={pipeline.description}>{pipeline.name}</option>
                )}
              </For>
              <option value="__new__">-- Create agent --</option>
            </select>
            <Show when={currentPipeline()}>
              <button
                class="delete-btn"
                onClick={() => props.onDeletePipeline?.(props.selectedPipeline)}
                title="Delete config"
              >✕</button>
              <button
                class="edit-btn"
                classList={{ modified: props.pipelineModified }}
                onClick={() => props.onEditPipeline?.()}
                disabled={!props.isConnected}
                title="Edit pipeline"
              >
                ✎
              </button>
            </Show>
          </div>
          <select
            class="model-select"
            value={props.selectedModel}
            onChange={(e) => props.onModelChange(e.currentTarget.value)}
            disabled={selectorsDisabled()}
            title="Default Model"
          >
            <option value="none">-- Unload GPU --</option>
            <For each={props.models}>
              {(model) => <option value={model.id}>{model.name}</option>}
            </For>
          </select>
        </div>
        <a href="/observe" class="observe-btn" title="Observe">
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 12h-4l-3 9L9 3l-3 9H2" />
          </svg>
        </a>
        <Settings />
      </header>
    </div>
  );
}
