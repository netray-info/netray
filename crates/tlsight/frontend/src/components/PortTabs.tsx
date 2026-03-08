import { For } from 'solid-js';

interface Props {
  ports: number[];
  selected: number;
  onSelect: (port: number) => void;
}

export default function PortTabs(props: Props) {
  return (
    <div class="port-tabs">
      <For each={props.ports}>
        {(port) => (
          <button
            class={`port-tabs__tab ${port === props.selected ? 'port-tabs__tab--active' : ''}`}
            onClick={() => props.onSelect(port)}
          >
            :{port}
          </button>
        )}
      </For>
    </div>
  );
}
