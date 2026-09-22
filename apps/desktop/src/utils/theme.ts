import { loadTheme, type Theme } from './preferences'

type ThemeRoot = Pick<HTMLElement, 'dataset'>

function documentRoot(): ThemeRoot | null {
  return typeof document === 'undefined' ? null : document.documentElement
}

export function applyTheme(theme: Theme, root = documentRoot()): void {
  if (root) {
    root.dataset.theme = theme
  }
}

export function initializeTheme(): void {
  applyTheme(loadTheme())
}
