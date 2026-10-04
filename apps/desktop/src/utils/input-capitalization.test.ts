import { parse } from 'vue/compiler-sfc'
import { expect, test } from 'vitest'

const components = import.meta.glob<string>('../**/*.vue', {
  query: '?raw',
  import: 'default',
  eager: true
})

test('text entry fields do not enable automatic capitalization or correction', () => {
  const missing: string[] = []
  let checked = 0

  for (const [path, source] of Object.entries(components)) {
    const template = parse(source).descriptor.template?.content ?? ''
    for (const match of template.matchAll(/<(input|textarea)\b([^>]*)>/gs)) {
      const [, tag, attributes] = match
      const type = /\btype="([^"]+)"/.exec(attributes)?.[1] ?? 'text'
      if (tag === 'input' && (type === 'checkbox' || type === 'number')) {
        continue
      }

      checked += 1
      if (!/\bautocapitalize="none"/.test(attributes) || !/\bautocorrect="off"/.test(attributes)) {
        missing.push(`${path}: ${tag} ${attributes.trim().split('\n')[0]}`)
      }
    }
  }

  expect(checked).toBeGreaterThan(0)
  expect(missing).toEqual([])
})
