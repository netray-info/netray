import { render, ErrorBoundary } from 'solid-js/web';
import App from './App';
import '@netray-info/common-frontend/styles/reset.css';
import '@netray-info/common-frontend/styles/theme.css';
import '@netray-info/common-frontend/styles/layout.css';
import '@netray-info/common-frontend/styles/components.css';
import './styles/global.css';

const root = document.getElementById('root');
if (root) {
  render(
    () => (
      <ErrorBoundary fallback={(err) => (
        <div class="error-banner">Something went wrong: {err.message}</div>
      )}>
        <App />
      </ErrorBoundary>
    ),
    root,
  );
}
