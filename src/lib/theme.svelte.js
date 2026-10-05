const mq = window.matchMedia('(prefers-color-scheme: dark)');

export const theme = $state({
  name: localStorage.getItem('theme') || 'default',
  mode: localStorage.getItem('mode') || 'system'
});

export function applyTheme() {
  const d = document.documentElement.dataset;
  d.theme = theme.name;
  d.mode = theme.mode === 'system' ? (mq.matches ? 'dark' : 'light') : theme.mode;
  localStorage.setItem('theme', theme.name);
  localStorage.setItem('mode', theme.mode);
}
mq.addEventListener('change', applyTheme);
