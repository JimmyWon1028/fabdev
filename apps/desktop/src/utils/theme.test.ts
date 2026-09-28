import { describe, expect, it, vi } from 'vitest'

import { supportedThemes } from './preferences'
import { applyTheme } from './theme'

describe('theme color mode', () => {
  it.each(supportedThemes)('applies and reverses both palettes for %s without changing the theme', (theme) => {
    const root = { dataset: {} as DOMStringMap }
    applyTheme(theme, 'dark', root)
    expect(root.dataset.theme).toBe(theme)
    expect(root.dataset.colorMode).toBe('dark')
    const darkVariant = root.dataset.themeVariant
    applyTheme(theme, 'light', root)
    expect(root.dataset.theme).toBe(theme)
    expect(root.dataset.colorMode).toBe('light')
    expect(root.dataset.themeVariant).not.toBe(darkVariant)
  })

  it('does not require a document during non-browser rendering', () => {
    expect(() => applyTheme('default', 'dark', null)).not.toThrow()
  })

  it('keeps Default dark palette overrides from changing component geometry', async () => {
    const { readFileSync } = await vi.importActual<{
      readFileSync: (path: URL, encoding: 'utf8') => string
    }>('node:fs')
    const colorModes = readFileSync(new URL('../themes/color-modes.css', import.meta.url), 'utf8')
    const sharedSelector = 'html[data-theme-variant="alternate"]:not([data-theme="neo-brutalism"])'
    const geometryProperty = /^(?:border(?:-(?:top|right|bottom|left))?(?:-(?:width|style|radius))?|border-.+-radius|padding(?:-.+)?|margin(?:-.+)?|(?:min-|max-)?(?:width|height)|display|position|inset|top|right|bottom|left|gap|(?:row|column)-gap|font(?:-.+)?|line-height|letter-spacing|grid-.+|flex(?:-.+)?|transform)$/
    let checkedRules = 0
    for (const [, selectors, body] of colorModes.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      if (!selectors.includes(sharedSelector) || selectors.includes(':not([data-theme="default"])')) {
        continue
      }
      checkedRules += 1
      for (const declaration of body.split(';')) {
        const property = declaration.split(':')[0].trim()
        expect(property, selectors.trim()).not.toMatch(geometryProperty)
      }
    }
    expect(checkedRules).toBeGreaterThan(0)
  })
})
