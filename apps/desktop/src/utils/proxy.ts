import type { ProxyConnectionInfo } from '@fabdev/contracts'

//2026.10.06 modify by codex, reason: derive the default by prefixing 4 without wrapping invalid ports.
export function defaultProxyHttpsPort(httpPort: number): number | null {
  const port = Number(`4${httpPort}`)
  return Number.isInteger(httpPort) && httpPort >= 1024 && httpPort <= 65535 && port <= 65535 ? port : null
}

export interface ProxyRemovalFailure {
  id: string
  message: string
}

export function filterProxyConnections(
  connections: ProxyConnectionInfo[],
  searchQuery: string
): ProxyConnectionInfo[] {
  const query = searchQuery.trim().toLocaleLowerCase()
  if (!query) {
    return connections
  }

  return connections.filter((connection) => [
    connection.id,
    connection.name,
    connection.domain,
    connection.listenHost,
    String(connection.listenPort),
    `${connection.listenHost}:${connection.listenPort}`,
    `http://${connection.domain}:${connection.listenPort}`,
    ...(connection.secured ? [`https://${connection.domain}:${connection.httpsPort}`] : []),
    connection.target,
    ...connection.allowedOrigins
  ].some((value) => value.toLocaleLowerCase().includes(query)))
}

export interface ProxyRemovalResult {
  removed: string[]
  failed: ProxyRemovalFailure[]
}

export async function removeProxyConnections(
  connectionIds: Iterable<string>,
  remove: (connectionId: string) => Promise<unknown>
): Promise<ProxyRemovalResult> {
  const result: ProxyRemovalResult = { removed: [], failed: [] }

  for (const connectionId of new Set(connectionIds)) {
    try {
      await remove(connectionId)
      result.removed.push(connectionId)
    } catch (error) {
      result.failed.push({
        id: connectionId,
        message: error instanceof Error ? error.message : String(error)
      })
    }
  }

  return result
}
