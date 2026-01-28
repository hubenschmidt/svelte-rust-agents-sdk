import { Show, createMemo } from 'solid-js';
import { marked } from 'marked';
import type { WsMetadata } from '../types';

type Props = {
  user: 'User' | 'Bot';
  msg: string;
  streaming?: boolean;
  metadata?: WsMetadata;
};

marked.setOptions({ breaks: true, gfm: true });

const formatMetadata = (m: WsMetadata): string => {
  const secs = (m.elapsed_ms / 1000).toFixed(1);

  if (m.tokens_per_sec !== undefined) {
    return `${secs}s · ${m.tokens_per_sec.toFixed(1)} tok/s · ${m.output_tokens} tokens`;
  }

  if (m.input_tokens > 0 || m.output_tokens > 0) {
    return `${secs}s · ${m.input_tokens}/${m.output_tokens} tokens`;
  }

  return `${secs}s`;
};

export default function ChatMessage(props: Props) {
  const showMetadata = () => props.user === 'Bot' && props.metadata && !props.streaming;
  const html = createMemo(() => marked.parse(props.msg) as string);

  return (
    <div
      class="message"
      classList={{
        user: props.user === 'User',
        bot: props.user === 'Bot',
        streaming: props.streaming
      }}
    >
      <div innerHTML={html()} />
      <Show when={showMetadata()}>
        <div class="metadata">{formatMetadata(props.metadata!)}</div>
      </Show>
    </div>
  );
}
