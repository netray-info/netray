import { For } from 'solid-js';

interface Props {
  ports: number[];
  selected: number;
  onSelect: (port: number) => void;
}

export default function PortTabs(props: Props) {
  const handleKeyDown = (e: KeyboardEvent) => {
    const currentIdx = props.ports.indexOf(props.selected);
    if (e.key === 'ArrowRight') {
      e.preventDefault();
      const next = props.ports[(currentIdx + 1) % props.ports.length];
      props.onSelect(next);
    } else if (e.key === 'ArrowLeft') {
      e.preventDefault();
      const prev = props.ports[(currentIdx - 1 + props.ports.length) % props.ports.length];
      props.onSelect(prev);
    }
  };

  return (
    <div class="port-tabs" role="tablist" onKeyDown={handleKeyDown}>
      <For each={props.ports}>
        {(port) => (
          <button
            role="tab"
            id={`tab-${port}`}
            aria-controls={`panel-${port}`}
            aria-selected={port === props.selected ? 'true' : 'false'}
            tabindex={port === props.selected ? 0 : -1}
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
