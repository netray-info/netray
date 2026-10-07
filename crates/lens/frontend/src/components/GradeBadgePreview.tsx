interface Props {
  grade: string;
  color: string;
}

export default function GradeBadgePreview(props: Props) {
  const text = () => (props.grade === 'error' ? '?' : props.grade);
  const labelW = 38; // "lens" = 4 chars × 7 + 10
  const valueW = () => text().length * 7 + 10;
  const totalW = () => labelW + valueW();
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width={totalW()} height="20" role="img" aria-hidden="true">
      <rect rx="3" width={totalW()} height="20" fill="#555"/>
      <rect rx="3" x={labelW} width={valueW()} height="20" style={{ fill: props.color }}/>
      <rect x={labelW} width="4" height="20" style={{ fill: props.color }}/>
      <rect rx="3" width="4" height="20" fill="#555"/>
      <g fill="#fff" text-anchor="middle" font-family="Verdana,Geneva,sans-serif" font-size="11">
        <text x={labelW / 2} y="14">lens</text>
        <text x={labelW + valueW() / 2} y="14">{text()}</text>
      </g>
    </svg>
  );
}
