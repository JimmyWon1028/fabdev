import type {
  SiteDiagnosticCheckKind,
  SiteDiagnosticReport,
  SiteDiagnosticStatus
} from '@fabdev/contracts'

export interface SiteDiagnosticCounts {
  passed: number
  warning: number
  failed: number
}

export interface SiteDiagnosticReportLabels {
  title: string
  site: string
  domain: string
  summary: (counts: SiteDiagnosticCounts) => string
  checks: string
  recentLogs: string
  noRecentLogs: string
  check: (kind: SiteDiagnosticCheckKind) => string
  status: (status: SiteDiagnosticStatus) => string
}

export function countSiteDiagnosticStatuses(
  report: SiteDiagnosticReport
): SiteDiagnosticCounts {
  return report.checks.reduce<SiteDiagnosticCounts>((counts, check) => {
    counts[check.status] += 1
    return counts
  }, { passed: 0, warning: 0, failed: 0 })
}

export function formatSiteDiagnosticReport(
  report: SiteDiagnosticReport,
  labels: SiteDiagnosticReportLabels
): string {
  const counts = countSiteDiagnosticStatuses(report)
  const lines = [
    labels.title,
    `${labels.site}: ${report.siteName}`,
    `${labels.domain}: ${report.domain}`,
    labels.summary(counts),
    '',
    labels.checks
  ]
  for (const check of report.checks) {
    lines.push(
      `- [${labels.status(check.status)}] ${labels.check(check.kind)}`
      + (check.detail ? `: ${check.detail}` : '')
    )
  }
  lines.push('', labels.recentLogs)
  if (report.recentLogs.length === 0) {
    lines.push(labels.noRecentLogs)
  } else {
    lines.push(...report.recentLogs.map((entry) => `[${entry.source}] ${entry.line}`))
  }
  return `${lines.join('\n')}\n`
}
