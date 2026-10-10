import type { SummaryEvent } from '../lib/types';

interface GradeDisplayProps {
  summary: SummaryEvent;
}

export default function GradeDisplay(props: GradeDisplayProps) {
  const gradeLabel = (): string => {
    const g = props.summary.grade as string;
    return g === 'skipped' ? 'Skipped' : g;
  };
  const gradeModifier = (): string => {
    const g = props.summary.grade as string;
    return g === 'skipped' ? 'skipped' : g.toLowerCase();
  };

  return (
    <div class="overview__item overview__item--grade">
      <span class="overview__label">Grade</span>
      <span
        class={`overview__grade overview__grade--${gradeModifier()}`}
        aria-label={`Grade ${gradeLabel()}`}
      >{gradeLabel()}</span>
    </div>
  );
}
