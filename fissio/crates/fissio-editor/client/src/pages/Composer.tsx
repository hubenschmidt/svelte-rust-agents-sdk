import { Show } from 'solid-js';
import { useNavigate } from '@solidjs/router';
import { chat } from '../lib/stores/chat';
import PipelineEditor from '../lib/components/PipelineEditor';
import type { PipelineInfo } from '../lib/types';

export default function Composer() {
  const navigate = useNavigate();

  const handlePipelineUpdate = (config: PipelineInfo) => {
    chat.setPipelineConfig(config);
  };

  const handleSavePipeline = async (config: PipelineInfo) => {
    await chat.savePipeline(config);
    chat.setSelectedPipeline(config.id);
    navigate('/');
  };

  return (
    <Show
      when={chat.pipelineConfig()}
      fallback={
        <div class="no-config">
          <p>No agent config loaded</p>
          <button onClick={() => navigate('/')}>Back to Chat</button>
        </div>
      }
    >
      {(config) => (
        <PipelineEditor
          config={config()}
          models={chat.models()}
          templates={chat.templates()}
          availableTools={chat.availableTools()}
          onUpdate={handlePipelineUpdate}
          onSave={handleSavePipeline}
        />
      )}
    </Show>
  );
}
