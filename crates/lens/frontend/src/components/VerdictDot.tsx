import type { Verdict } from '../lib/types';

function gradeClass(grade: string): string {
  switch (grade) {
    case 'A+':
    case 'A':  return 'verdict-dot--grade-a';
    case 'B':  return 'verdict-dot--grade-b';
    case 'C':  return 'verdict-dot--grade-c';
    case 'D':  return 'verdict-dot--grade-d';
    default:   return 'verdict-dot--grade-f';
  }
}

interface Props {
  verdict: Verdict;
  grade?: string;
}

export default function VerdictDot(props: Props) {
  const dotClass = () =>
    props.grade !== undefined && props.verdict !== 'error'
      ? `verdict-dot ${gradeClass(props.grade)}`
      : `verdict-dot verdict-dot--${props.verdict}`;

  return (
    <span
      class={dotClass()}
      aria-label={props.grade ?? props.verdict}
      role="img"
    />
  );
}
