async function loadPlugin(name: string) {
  const mod = await import('./plugins/core');
  return mod.default;
}

async function loadTheme() {
  const theme = await import('./themes/dark');
  return theme;
}
