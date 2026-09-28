import { defaultThemeMode, loadTheme, loadThemeMode, type Theme, type ThemeMode } from './preferences'

type ThemeRoot = Pick<HTMLElement, 'dataset'>

function documentRoot(): ThemeRoot | null {
  return typeof document === 'undefined' ? null : document.documentElement
}

export function applyTheme(theme: Theme, mode: ThemeMode, root = documentRoot()): void {
  if (root) {
    root.dataset.theme = theme
    root.dataset.colorMode = mode
    root.dataset.themeVariant = mode === defaultThemeMode(theme) ? 'original' : 'alternate'
  }
}

export function initializeTheme(): void {
  const theme = loadTheme()
  applyTheme(theme, loadThemeMode(theme))
}
