// "Want to look deeper? → Raw data for every check" — the SDD §3 Requirement 13
// callout that points users at the engineer-tier inspectors. Persisted across
// idle and result states so a user who got HTTP F has a visible path to the
// raw inspector data.
export default function DeeperCallout() {
  return (
    <p class="deeper-callout">
      Want to look deeper? →{' '}
      <a class="deeper-callout__link" href="/tools">Raw data for every check</a>
    </p>
  );
}
