import type { JSX } from 'solid-js';

type Props = {
  value: string;
  onValueChange: (v: string) => void;
  disabled?: boolean;
  sendDisabled?: boolean;
  onSend: () => void;
};

export default function ChatInput(props: Props) {
  const handleKeydown: JSX.EventHandler<HTMLTextAreaElement, KeyboardEvent> = (event) => {
    if (event.key !== 'Enter') return;
    if (event.shiftKey) return;
    event.preventDefault();
    props.onSend();
  };

  return (
    <div class="input-area">
      <textarea
        value={props.value}
        onInput={(e) => props.onValueChange(e.currentTarget.value)}
        onKeyDown={handleKeydown}
        placeholder="Type a message..."
        disabled={props.disabled}
        rows="1"
      />
      <button onClick={props.onSend} disabled={props.sendDisabled}>
        Send
      </button>
    </div>
  );
}
