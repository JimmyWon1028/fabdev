export const protocolVersion = 37

export type ServiceState =
  | 'notInstalled'
  | 'installed'
  | 'starting'
  | 'running'
  | 'stopping'
  | 'stopped'
  | 'updating'
  | 'failed'

export interface AgentStatus {
  protocolVersion: number
  agentVersion: string
  dns: ServiceState
  nginx: ServiceState
  phpFpm: ServiceState
  phpFpmPools: PhpFpmPoolStatus[]
  mariadb: ServiceState
}

export interface PhpFpmPoolStatus {
  version: string
  activeProcesses: number
  idleProcesses: number
  totalProcesses: number
  listenQueue: number
  maxListenQueue: number
  maxChildrenReached: number
  slowRequests: number
}

export interface Site {
  id: string
  name: string
  domain: string
  projectPath: string
  documentRoot: string
  phpVersion: string | null
  enabled: boolean
  secured: boolean
}

export interface SiteInput {
  name?: string
  domain?: string
  projectPath: string
  documentRoot?: string
  phpVersion: string | null
}

export interface SiteEditInput {
  name: string
  domain: string
  projectPath: string
  documentRoot?: string
}

export interface SiteHomeSettings {
  path: string
  siteIds: string[]
  symbolicLinkSiteIds: string[]
}

export interface LanShareInfo {
  host: string
  port: number
  sites: LanShareSiteInfo[]
}

export interface LanShareSiteInfo {
  siteId: string
  domain: string
}

export interface PhpRuntimeInfo {
  version: string
  series: string
  active: boolean
  sites: string[]
}

export interface PhpRuntimeState {
  globalVersion: string | null
  installed: PhpRuntimeInfo[]
}

export interface TerminalPhpState {
  enabled: boolean
  binPath: string
  shimPath: string
}

export interface NodeRuntimeInfo {
  version: string
  active: boolean
}

export interface TerminalNodeState {
  enabled: boolean
  binPath: string
  shimPaths: string[]
}

export interface NodeRuntimeState {
  activeVersion: string | null
  installed: NodeRuntimeInfo[]
  terminal: TerminalNodeState
}

export interface RuntimeUpdateCheck {
  catalogSequence: number
  generatedAt: string
  expiresAt: string
  unsignedCommunityBuild: boolean
  artifacts: RuntimeUpdateArtifact[]
}

export interface RuntimeUpdateArtifact {
  name: string
  version: string
  platform: string
  architecture: string
  minimumOsVersion: string
  fileName: string
  size: number
  sha256: string
  unsignedCommunityBuild: boolean
  installed: boolean
  packageUpdateAvailable: boolean
  activeVersion: string | null
}

export type RuntimeUpdateOperationStatus =
  | 'queued'
  | 'downloading'
  | 'verified'
  | 'installing'
  | 'completed'
  | 'failed'
  | 'cancelled'

export interface RuntimeUpdateOperation {
  operationId: string
  status: RuntimeUpdateOperationStatus
  name: string
  version: string
  platform: string
  architecture: string
  fileName: string
  bytesDownloaded: number
  totalBytes: number
  sha256: string
  error: string | null
}

export type ProxyConnectionState =
  | 'starting'
  | 'running'
  | 'degraded'
  | 'stopping'
  | 'stopped'
  | 'failed'

export interface ProxyConnectionInfo {
  id: string
  name: string
  domain: string
  listenHost: string
  listenPort: number
  target: string
  allowedOrigins: string[]
  state: ProxyConnectionState
  lastError: string | null
}

export interface ProxyConnectionInput {
  id: string
  domain: string
  listenPort: number
  target: string
  allowedOrigins: string[]
}

export interface ProxyManagerState {
  connections: ProxyConnectionInfo[]
}

export interface MariaDbSettings {
  port: number
  dataDir: string
  connectionMode: MariaDbConnectionMode
  systemSocket: string
}

export type MariaDbConnectionMode = 'managed' | 'system'

export interface MariaDbConfig {
  filename: string
  contents: string
}

export interface LocalCaInfo {
  certificatePath: string
  fingerprintSha256: string
}

export type AgentRequest =
  | { type: 'ping' }
  | { type: 'getStatus' }
  | { type: 'listSites' }
  | { type: 'getSiteHome' }
  | { type: 'saveSiteHome'; payload: { path: string } }
  | { type: 'addSite'; payload: SiteInput }
  | { type: 'updateSite'; payload: { siteId: string; input: SiteEditInput } }
  | { type: 'removeSite'; payload: { siteId: string } }
  | { type: 'setSitePhp'; payload: { siteId: string; phpVersion: string | null } }
  | { type: 'ensureLocalCa' }
  | { type: 'setSiteHttps'; payload: { siteId: string; secured: boolean } }
  | { type: 'getLanShare' }
  | { type: 'startLanShare'; payload: { siteId: string; port: number } }
  | { type: 'stopLanShareSite'; payload: { siteId: string } }
  | { type: 'stopLanShare' }
  | { type: 'checkRuntimeUpdates' }
  | { type: 'startRuntimeDownload'; payload: { name: string; version: string } }
  | { type: 'getRuntimeUpdateOperation'; payload: { operationId: string } }
  | { type: 'cancelRuntimeDownload'; payload: { operationId: string } }
  | { type: 'installDownloadedRuntime'; payload: { operationId: string } }
  | { type: 'listPhpRuntimes' }
  | {
      type: 'installPhpRuntime'
      payload: { artifactPath: string; releasePath: string }
    }
  | { type: 'setGlobalPhp'; payload: { version: string } }
  | { type: 'getTerminalPhp' }
  | { type: 'enableTerminalPhp' }
  | { type: 'disableTerminalPhp' }
  | { type: 'removePhpRuntime'; payload: { version: string } }
  | { type: 'getPhpIni'; payload: { phpVersion: string } }
  | { type: 'savePhpIni'; payload: { phpVersion: string; contents: string } }
  | { type: 'getDefaultPhpIni' }
  | { type: 'saveDefaultPhpIni'; payload: { contents: string } }
  | { type: 'getErpPhpIni'; payload: { phpVersion: string | null } }
  | { type: 'getNodeRuntime' }
  | {
      type: 'installNodeRuntime'
      payload: { artifactPath: string; releasePath: string }
    }
  | { type: 'setGlobalNode'; payload: { version: string } }
  | { type: 'enableTerminalNode' }
  | { type: 'disableTerminalNode' }
  | { type: 'removeNodeRuntime'; payload: { version: string } }
  | { type: 'getProxyManager' }
  | { type: 'addProxyConnection'; payload: ProxyConnectionInput }
  | {
      type: 'updateProxyConnection'
      payload: { connectionId: string; input: ProxyConnectionInput }
    }
  | { type: 'removeProxyConnection'; payload: { connectionId: string } }
  | { type: 'startProxyConnection'; payload: { connectionId: string } }
  | { type: 'stopProxyConnection'; payload: { connectionId: string } }
  | { type: 'startAllProxyConnections' }
  | { type: 'stopAllProxyConnections' }
  | { type: 'shutdown' }
  | { type: 'startAll' }
  | { type: 'stopAll' }
  | { type: 'startMariaDb' }
  | { type: 'stopMariaDb' }
  | { type: 'restoreMariaDbLastState' }
  | { type: 'getMariaDbSettings' }
  | { type: 'saveMariaDbSettings'; payload: MariaDbSettings }
  | { type: 'getMariaDbConfig' }
  | { type: 'saveMariaDbConfig'; payload: { contents: string } }
  | {
      type: 'setMariaDbRootPassword'
      payload: { currentPassword: string; newPassword: string }
    }
  | {
      type: 'installMariaDbRuntime'
      payload: { artifactPath: string; releasePath: string }
    }
  | { type: 'removeMariaDbRuntime' }

export type AgentResponse =
  | { type: 'pong'; payload: { protocolVersion: number } }
  | { type: 'status'; payload: AgentStatus }
  | { type: 'sites'; payload: Site[] }
  | { type: 'siteHomeSettings'; payload: SiteHomeSettings }
  | { type: 'siteAdded'; payload: Site }
  | { type: 'siteUpdated'; payload: Site }
  | { type: 'siteRemoved'; payload: Site }
  | { type: 'sitePhpChanged'; payload: Site }
  | { type: 'localCaReady'; payload: LocalCaInfo }
  | { type: 'siteHttpsChanged'; payload: Site }
  | { type: 'lanShare'; payload: LanShareInfo | null }
  | { type: 'runtimeUpdates'; payload: RuntimeUpdateCheck }
  | { type: 'runtimeUpdateOperation'; payload: RuntimeUpdateOperation }
  | { type: 'phpRuntimes'; payload: PhpRuntimeState }
  | { type: 'phpRuntimeInstalled'; payload: PhpRuntimeState }
  | { type: 'globalPhpChanged'; payload: PhpRuntimeState }
  | { type: 'terminalPhp'; payload: TerminalPhpState }
  | { type: 'phpRuntimeRemoved'; payload: PhpRuntimeState }
  | { type: 'phpIni'; payload: { phpVersion: string; contents: string } }
  | { type: 'phpIniSaved'; payload: { phpVersion: string } }
  | { type: 'defaultPhpIni'; payload: { contents: string } }
  | { type: 'defaultPhpIniSaved' }
  | { type: 'erpPhpIni'; payload: { phpVersion: string | null; contents: string } }
  | { type: 'nodeRuntime'; payload: NodeRuntimeState }
  | { type: 'nodeRuntimeInstalled'; payload: NodeRuntimeState }
  | { type: 'globalNodeChanged'; payload: NodeRuntimeState }
  | { type: 'terminalNode'; payload: NodeRuntimeState }
  | { type: 'nodeRuntimeRemoved'; payload: NodeRuntimeState }
  | { type: 'proxyManager'; payload: ProxyManagerState }
  | { type: 'started' }
  | { type: 'stopped' }
  | { type: 'mariaDbStarted' }
  | { type: 'mariaDbStopped' }
  | { type: 'mariaDbStateRestored' }
  | { type: 'mariaDbSettings'; payload: MariaDbSettings }
  | { type: 'mariaDbConfig'; payload: MariaDbConfig }
  | { type: 'mariaDbConfigSaved'; payload: MariaDbConfig }
  | { type: 'mariaDbRootPasswordChanged' }
  | { type: 'mariaDbRuntimeInstalled'; payload: { version: string } }
  | { type: 'mariaDbRuntimeRemoved'; payload: { version: string } }
  | { type: 'error'; payload: { code: string; message: string } }
