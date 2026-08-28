import { describe, expect, it } from 'vitest'

import { getOperationManual, isHelpShortcut } from './help'

describe('operation manual', () => {
  it('provides the full guide in every supported language', () => {
    for (const language of ['en', 'zh-TW', 'zh-CN'] as const) {
      const manual = getOperationManual(language)
      expect(manual.sections.map((section) => section.id)).toEqual([
        'quick-start',
        'overview',
        'sites',
        'php',
        'mariadb',
        'nodejs',
        'proxy',
        'settings-shutdown',
        'troubleshooting'
      ])
      expect(manual.sections.every((section) => section.summary.length > 0)).toBe(true)
    }
  })

  it('opens only for an unmodified F1 keypress', () => {
    const event = {
      key: 'F1',
      altKey: false,
      ctrlKey: false,
      metaKey: false,
      shiftKey: false
    }

    expect(isHelpShortcut(event)).toBe(true)
    expect(isHelpShortcut({ ...event, key: 'Escape' })).toBe(false)
    expect(isHelpShortcut({ ...event, metaKey: true })).toBe(false)
  })
})
