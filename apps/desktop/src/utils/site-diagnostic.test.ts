import type { SiteDiagnosticReport } from '@fabdev/contracts'
import { describe, expect, it } from 'vitest'

import { countSiteDiagnosticStatuses, formatSiteDiagnosticReport } from './site-diagnostic'

const report: SiteDiagnosticReport = {
  siteId: 'site-one',
  siteName: 'Site One',
  domain: 'site-one.test',
  checks: [
    { kind: 'dns', status: 'passed', detail: '127.0.0.1' },
    { kind: 'http', status: 'warning', detail: 'HTTP 404' },
    { kind: 'php', status: 'failed', detail: 'PHP 8.2' }
  ],
  recentLogs: [{ source: 'nginx-error.log', line: 'site-one.test failed' }]
}

describe('Site diagnostic reports', () => {
  it('counts each diagnostic status', () => {
    expect(countSiteDiagnosticStatuses(report)).toEqual({
      passed: 1,
      warning: 1,
      failed: 1
    })
  })

  it('formats a copyable report without adding local paths', () => {
    const formatted = formatSiteDiagnosticReport(report, {
      title: 'fabDev Site diagnostic report',
      site: 'Site',
      domain: 'Domain',
      summary: ({ passed, warning, failed }) =>
        `Passed ${passed} · Warnings ${warning} · Failed ${failed}`,
      checks: 'Checks',
      recentLogs: 'Recent logs',
      noRecentLogs: 'No relevant logs',
      check: (kind) => kind,
      status: (status) => status
    })

    expect(formatted).toContain('[passed] dns: 127.0.0.1')
    expect(formatted).toContain('[nginx-error.log] site-one.test failed')
    expect(formatted).not.toContain('/Users/')
  })
})
