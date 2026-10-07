import { render } from 'solid-js/web'
import { ErrorBoundary } from 'solid-js'
import App from './App'
import './styles/global.css'

render(
  () => (
    <ErrorBoundary fallback={(err) => <div role="alert" style="padding:2rem;color:var(--fail)">{String(err)}</div>}>
      <App />
    </ErrorBoundary>
  ),
  document.getElementById('root')!
)
