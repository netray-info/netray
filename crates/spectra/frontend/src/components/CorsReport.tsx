import type { CorsReport as CorsReportType } from '../lib/types';

interface Props {
  cors: CorsReportType;
}

export default function CorsReport(props: Props) {
  return (
    <>
      <p class="cors-message">
        {props.cors.message}
      </p>
      <ul class="cors-flags-list">
        <li>Allows any origin: {props.cors.allows_any_origin ? 'Yes' : 'No'}</li>
        <li>Reflects origin: {props.cors.reflects_origin ? 'Yes' : 'No'}</li>
        <li>Allows credentials: {props.cors.allows_credentials ? 'Yes' : 'No'}</li>
      </ul>
    </>
  );
}
