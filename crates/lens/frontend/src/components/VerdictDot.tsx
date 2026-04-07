import type { Verdict } from '../lib/types';

interface Props {
  verdict: Verdict;
}

export default function VerdictDot(props: Props) {
  return (
    <span
      class={`verdict-dot verdict-dot--${props.verdict}`}
      aria-label={props.verdict}
      role="img"
    />
  );
}
