import type {
  AgentStatus,
  ProxyConnectionInfo,
  ProxyConnectionState,
  ServiceState,
  Site
} from '@fabdev/contracts'

export type ProxySummaryState = Extract<
  ProxyConnectionState,
  'starting' | 'running' | 'degraded' | 'stopping' | 'stopped' | 'failed'
>

export interface ProxyConnectionSummary {
  total: number
  running: number
  issues: number
  state: ProxySummaryState
}

const serviceStates = (status: AgentStatus): ServiceState[] => [
  status.dns,
  status.nginx,
  status.phpFpm
]

export function areAllServicesRunning(status: AgentStatus): boolean {
  return serviceStates(status).every((state) => state === 'running')
}

export function shouldStopServicesBeforeStart(status: AgentStatus): boolean {
  return serviceStates(status).some((state) =>
    ['starting', 'running', 'stopping', 'failed'].includes(state)
  )
}

export function hasEnabledSites(sites: Array<Pick<Site, 'enabled'>>): boolean {
  return sites.some((site) => site.enabled)
}

export function canToggleAllServices(
  busy: boolean,
  allServicesRunning: boolean,
  enabledSites: boolean
): boolean {
  return !busy && (allServicesRunning || enabledSites)
}

export function summarizeProxyConnections(
  connections: Array<Pick<ProxyConnectionInfo, 'state'>>
): ProxyConnectionSummary {
  const count = (state: ProxyConnectionState) =>
    connections.filter((connection) => connection.state === state).length
  const running = count('running')
  const degraded = count('degraded')
  const failed = count('failed')

  let state: ProxySummaryState = 'stopped'
  if (failed > 0) {
    state = 'failed'
  } else if (degraded > 0) {
    state = 'degraded'
  } else if (count('starting') > 0) {
    state = 'starting'
  } else if (count('stopping') > 0) {
    state = 'stopping'
  } else if (running > 0) {
    state = 'running'
  }

  return {
    total: connections.length,
    running,
    issues: degraded + failed,
    state
  }
}
