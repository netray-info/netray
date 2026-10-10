import { ErrorBoundary, render } from 'solid-js/web';
import App from './App';
import './styles/global.css';

const root = document.getElementById('root');
if (root) {
  render(
    () => (
      <ErrorBoundary
        fallback={(err: unknown) => (
          <div role="alert" class="app-error-boundary">
            <h2>Something went wrong</h2>
            <p>{err instanceof Error ? err.message : String(err)}</p>
            <button type="button" onClick={() => location.reload()}>
              Reload
            </button>
          </div>
        )}
      >
        <App />
      </ErrorBoundary>
    ),
    root,
  );
}
