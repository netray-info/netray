import type { Component } from 'solid-js';
import type { ThemeResult } from '../theme';

interface ThemeToggleProps {
  theme: ThemeResult;
  class?: string;
}

const ThemeToggle: Component<ThemeToggleProps> = (props) => {
  const label = () => {
    const t = props.theme.theme();
    return `Theme: ${t === 'dark' ? 'Dark' : t === 'light' ? 'Light' : 'System'}. Click to switch.`;
  };
  const icon = () => {
    const t = props.theme.theme();
    return t === 'system' ? '\u25D1' : t === 'dark' ? '\u263E' : '\u2600';
  };
  return (
    <button
      class={`theme-toggle${props.class ? ' ' + props.class : ''}`}
      onClick={props.theme.toggleTheme}
      title={label()}
      aria-label={label()}
    >
      {icon()}
    </button>
  );
};

export default ThemeToggle;
