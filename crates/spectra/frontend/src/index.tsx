import { render, ErrorBoundary } from 'solid-js/web';
import App from './App';
import './styles/global.css';

const root = document.getElementById('root');
if (root) {
  render(
    () => (
      <ErrorBoundary fallback={(err) => <div class="error-banner">Something went wrong: {err.message}</div>}>
        <App />
      </ErrorBoundary>
    ),
    root,
  );
}
