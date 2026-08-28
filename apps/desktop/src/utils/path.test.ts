import { describe, expect, it } from 'vitest'

import { formatPathForDisplay, isWindowsPlatform } from './path'

describe('formatPathForDisplay', () => {
  it('removes the Windows verbatim prefix and uses backslashes', () => {
    expect(formatPathForDisplay('\\\\?\\C:\\Users\\dev\\Sites', true))
      .toBe('C:\\Users\\dev\\Sites')
    expect(formatPathForDisplay('C:/Users/dev/Sites/demo', true))
      .toBe('C:\\Users\\dev\\Sites\\demo')
  })

  it('converts a verbatim UNC path to a regular UNC path', () => {
    expect(formatPathForDisplay('\\\\?\\UNC\\server\\share\\Sites', true))
      .toBe('\\\\server\\share\\Sites')
  })

  it('leaves paths unchanged outside Windows', () => {
    expect(formatPathForDisplay('/Users/dev/Sites', false)).toBe('/Users/dev/Sites')
  })
})

describe('isWindowsPlatform', () => {
  it('detects a Windows user agent', () => {
    expect(isWindowsPlatform('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')).toBe(true)
    expect(isWindowsPlatform('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')).toBe(false)
  })
})
