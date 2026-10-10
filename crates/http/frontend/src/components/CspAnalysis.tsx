import { Show, For } from 'solid-js';
import type { CspReport } from '../lib/types';

interface Props {
  csp: CspReport;
}

export default function CspAnalysis(props: Props) {
  const directives = () => Object.entries(props.csp.directives);

  const issuesFor = (name: string) =>
    props.csp.issues.filter(i => i.toLowerCase().includes(name.toLowerCase()));

  const assignedIssues = () => {
    const s = new Set<string>();
    for (const [name] of directives()) {
      for (const issue of issuesFor(name)) s.add(issue);
    }
    return s;
  };

  const topLevelIssues = () => props.csp.issues.filter(i => !assignedIssues().has(i));

  return (
    <>
      <Show when={topLevelIssues().length > 0}>
        <ul class="csp-issues">
          <For each={topLevelIssues()}>
            {(issue) => <li class="csp-issue">{issue}</li>}
          </For>
        </ul>
      </Show>

      <Show when={directives().length > 0}>
        <ul class="csp-directives">
          <For each={directives()}>
            {([name, values]) => (
              <li class="csp-directive">
                <span class="csp-directive__name">{name}</span>
                <span class="csp-directive__values">{values.join(' ')}</span>
                <For each={issuesFor(name)}>
                  {(issue) => <span class="csp-directive__issue">{issue}</span>}
                </For>
              </li>
            )}
          </For>
        </ul>
      </Show>
    </>
  );
}
