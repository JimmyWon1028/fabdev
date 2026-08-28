import { describe, expect, it } from 'vitest'

import { translate, translateError } from './i18n'

describe('translations', () => {
  it('returns each supported language', () => {
    expect(translate('settings.title', {}, 'en')).toBe('Settings')
    expect(translate('settings.title', {}, 'zh-TW')).toBe('設定')
    expect(translate('settings.title', {}, 'zh-CN')).toBe('设置')
  })

  it('interpolates dynamic values', () => {
    expect(translate('sites.switched', { domain: 'demo.test', version: '8.2' }, 'en'))
      .toBe('demo.test was switched to PHP 8.2')
  })

  it('localizes the system ingress error and preserves its ports', () => {
    const message = 'system ingress is unavailable on DNS port 53, HTTP port 80, or HTTPS port 443'

    expect(translateError(message, 'zh-TW')).toBe(
      '系統入口無法使用 DNS 連接埠 53、HTTP 連接埠 80 或 HTTPS 連接埠 443。'
    )
    expect(translateError(message, 'zh-CN')).toBe(
      '系统入口无法使用 DNS 端口 53、HTTP 端口 80 或 HTTPS 端口 443。'
    )
  })

  it('localizes MariaDB root password errors without exposing SQL', () => {
    expect(
      translateError(
        'unable to authenticate as MariaDB root; verify the current password',
        'zh-TW'
      )
    ).toBe('無法以 MariaDB root 身分驗證，請確認目前密碼。')
    expect(
      translateError('MariaDB rejected the root password change (error 1819)', 'zh-TW')
    ).toBe('MariaDB 拒絕變更 root 密碼（錯誤 1819）。')
  })

  it('keeps unknown technical errors unchanged', () => {
    expect(translateError('unexpected Agent error', 'zh-TW')).toBe('unexpected Agent error')
  })
})
