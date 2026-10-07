interface CrossLinkProps {
  href: string;
  label: string;
}

export default function CrossLink(props: CrossLinkProps) {
  return (
    <a class="cross-link" href={props.href}>
      {props.label} &rarr;
    </a>
  );
}
