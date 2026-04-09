import { For } from 'solid-js';

interface Props {
  headers: Record<string, string>;
}

export default function HeadersView(props: Props) {
  return (
    <table class="headers-table">
      <tbody>
        <For each={Object.entries(props.headers)}>
          {([name, value]) => (
            <tr>
              <td>{name}</td>
              <td>{value}</td>
            </tr>
          )}
        </For>
      </tbody>
    </table>
  );
}
