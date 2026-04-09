import type { CorsReport as CorsReportType } from '../lib/types';

interface Props {
  cors: CorsReportType;
}

export default function CorsReport(props: Props) {
  return (
    <div class="section">
      <div class="section__title">
        CORS
        <span class={`badge badge--${props.cors.status}`} style={{ 'margin-left': '0.5rem' }}>
          {props.cors.status}
        </span>
      </div>
      <p style={{ 'font-size': '0.875rem', 'margin-bottom': '0.5rem' }}>
        {props.cors.message}
      </p>
      <ul style={{ 'list-style': 'none', padding: '0', margin: '0', 'font-size': '0.8125rem' }}>
        <li>Allows any origin: {props.cors.allows_any_origin ? 'Yes' : 'No'}</li>
        <li>Reflects origin: {props.cors.reflects_origin ? 'Yes' : 'No'}</li>
        <li>Allows credentials: {props.cors.allows_credentials ? 'Yes' : 'No'}</li>
      </ul>
    </div>
  );
}
