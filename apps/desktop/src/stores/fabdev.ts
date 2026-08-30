import {
  stableNodeVersion,
  type AgentRequest,
  type AgentResponse,
  type AgentStatus,
  type LanShareInfo,
  type MariaDbConfig,
  type MariaDbSettings,
  type NodeRuntimeState,
  type PhpRuntimeState,
  type ProxyConnectionInput,
  type ProxyManagerState,
  type RuntimeUpdateCheck,
  type RuntimeUpdateOperation,
  type Site,
  type SiteEditInput,
  type SiteHomeSettings,
  type SiteInput,
  type TerminalPhpState
} from '@fabdev/contracts'
import { invoke } from '@tauri-apps/api/core'
import { defineStore } from 'pinia'

import type {
  AppUpdateCheck,
  AppUpdateDownloadProgress,
  DownloadedAppUpdate
} from '../utils/app-update'
import {
  loadAutoCheckUpdates,
  loadAutoStartServices,
  loadLastUpdateCheck,
  saveAutoCheckUpdates,
  saveAutoStartServices,
  saveLastUpdateCheck,
  shouldAutomaticallyCheckUpdates
} from '../utils/preferences'
import {
  areAllServicesRunning,
  hasEnabledSites,
  shouldStopServicesBeforeStart
} from '../utils/service'

interface StoreState {
  connected: boolean
  busy: boolean
  error: string | null
  autoStartServices: boolean
  autoCheckUpdates: boolean
  lastUpdateCheck: string | null
  appUpdateBusy: boolean
  appUpdateError: string | null
  appUpdate: AppUpdateCheck | null
  appUpdateDownload: AppUpdateDownloadProgress | null
  downloadedAppUpdate: DownloadedAppUpdate | null
  status: AgentStatus | null
  lanShare: LanShareInfo | null
  mariaDbConfig: MariaDbConfig | null
  mariaDbSettings: MariaDbSettings | null
  siteHome: SiteHomeSettings | null
  sites: Site[]
  phpRuntimes: PhpRuntimeState
  terminalPhp: TerminalPhpState | null
  runtimeUpdateCheck: RuntimeUpdateCheck | null
  runtimeUpdateOperation: RuntimeUpdateOperation | null
  nodeRuntime: NodeRuntimeState
  proxyManager: ProxyManagerState
}

async function sendRequest(request: AgentRequest): Promise<AgentResponse> {
  return invoke<AgentResponse>('agent_request', { request })
}

export const useAppStore = defineStore('fabdev', {
  state: (): StoreState => ({
    connected: false,
    busy: false,
    error: null,
    autoStartServices: loadAutoStartServices(),
    autoCheckUpdates: loadAutoCheckUpdates(),
    lastUpdateCheck: loadLastUpdateCheck(),
    appUpdateBusy: false,
    appUpdateError: null,
    appUpdate: null,
    appUpdateDownload: null,
    downloadedAppUpdate: null,
    status: null,
    lanShare: null,
    mariaDbConfig: null,
    mariaDbSettings: null,
    siteHome: null,
    sites: [],
    phpRuntimes: {
      globalVersion: null,
      installed: []
    },
    terminalPhp: null,
    runtimeUpdateCheck: null,
    runtimeUpdateOperation: null,
    nodeRuntime: {
      stableVersion: stableNodeVersion,
      installedVersion: null
    },
    proxyManager: {
      connections: []
    }
  }),
  actions: {
    async readConfigTransferFile(path: string) {
      return invoke<string>('read_config_transfer_file', { path })
    },
    async writeConfigTransferFile(path: string, contents: string) {
      return invoke<void>('write_config_transfer_file', { path, contents })
    },
    setError(message: string) {
      this.error = message
    },
    clearError() {
      this.error = null
    },
    setAutoStartServices(enabled: boolean) {
      saveAutoStartServices(enabled)
      this.autoStartServices = enabled
    },
    setAutoCheckUpdates(enabled: boolean) {
      saveAutoCheckUpdates(enabled)
      this.autoCheckUpdates = enabled
    },
    setAppUpdateDownloadProgress(progress: AppUpdateDownloadProgress) {
      this.appUpdateDownload = progress
    },
    async checkAppUpdate() {
      this.appUpdateBusy = true
      this.appUpdateError = null
      try {
        const update = await invoke<AppUpdateCheck>('check_app_update')
        const checkedAt = new Date().toISOString()
        this.appUpdate = update
        this.lastUpdateCheck = checkedAt
        saveLastUpdateCheck(checkedAt)
        if (!update.updateAvailable || this.downloadedAppUpdate?.version !== update.latestVersion) {
          this.downloadedAppUpdate = null
        }
        return update
      } catch (error) {
        this.appUpdateError = error instanceof Error ? error.message : String(error)
        throw error
      } finally {
        this.appUpdateBusy = false
      }
    },
    async checkAppUpdateOnLaunch() {
      if (
        !shouldAutomaticallyCheckUpdates(
          this.autoCheckUpdates,
          this.lastUpdateCheck
        )
      ) {
        return
      }
      try {
        await this.checkAppUpdate()
      } catch {
        // Update failures must not block normal App startup.
      }
    },
    async downloadAppUpdate() {
      this.appUpdateBusy = true
      this.appUpdateError = null
      this.appUpdateDownload = null
      try {
        const download = await invoke<DownloadedAppUpdate>('download_app_update')
        this.downloadedAppUpdate = download
        return download
      } catch (error) {
        this.appUpdateError = error instanceof Error ? error.message : String(error)
        throw error
      } finally {
        this.appUpdateBusy = false
      }
    },
    async openAppReleaseNotes() {
      const version = this.appUpdate?.latestVersion
      if (!version) {
        throw new Error('Check for updates before opening Release Notes')
      }
      await invoke<void>('open_app_release_notes', { version })
    },
    async installDownloadedAppUpdate() {
      this.appUpdateBusy = true
      this.appUpdateError = null
      try {
        return await invoke<DownloadedAppUpdate>('install_downloaded_app_update')
      } catch (error) {
        this.appUpdateError = error instanceof Error ? error.message : String(error)
        throw error
      } finally {
        this.appUpdateBusy = false
      }
    },
    async refreshStatus() {
      this.busy = true
      this.error = null
      try {
        const response = await sendRequest({ type: 'getStatus' })
        if (response.type === 'status') {
          this.status = response.payload
          this.connected = true
        } else if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
      } catch (error) {
        this.connected = false
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async pollStatus() {
      try {
        const response = await sendRequest({ type: 'getStatus' })
        if (response.type === 'status') {
          this.status = response.payload
          this.connected = true
        } else if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
      } catch {
        this.connected = false
      }
    },
    async loadSites() {
      const response = await sendRequest({ type: 'listSites' })
      if (response.type === 'sites') {
        this.sites = response.payload
        this.connected = true
      } else if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
    },
    async loadSiteHome() {
      const response = await sendRequest({ type: 'getSiteHome' })
      if (response.type === 'siteHomeSettings') {
        this.siteHome = response.payload
        this.connected = true
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async saveSiteHome(path: string) {
      const response = await sendRequest({
        type: 'saveSiteHome',
        payload: { path }
      })
      if (response.type === 'siteHomeSettings') {
        this.siteHome = response.payload
        await this.loadSites()
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadPhpRuntimes() {
      const response = await sendRequest({ type: 'listPhpRuntimes' })
      if (response.type === 'phpRuntimes') {
        this.phpRuntimes = response.payload
        this.connected = true
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadTerminalPhp() {
      const response = await sendRequest({ type: 'getTerminalPhp' })
      if (response.type === 'terminalPhp') {
        this.terminalPhp = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async enableTerminalPhp() {
      const response = await sendRequest({ type: 'enableTerminalPhp' })
      if (response.type === 'terminalPhp') {
        this.terminalPhp = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async disableTerminalPhp() {
      const response = await sendRequest({ type: 'disableTerminalPhp' })
      if (response.type === 'terminalPhp') {
        this.terminalPhp = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async checkRuntimeUpdates() {
      const response = await sendRequest({ type: 'checkRuntimeUpdates' })
      if (response.type === 'runtimeUpdates') {
        this.runtimeUpdateCheck = response.payload
        this.runtimeUpdateOperation = null
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async startRuntimeDownload(name: string, version: string) {
      const response = await sendRequest({
        type: 'startRuntimeDownload',
        payload: { name, version }
      })
      if (response.type === 'runtimeUpdateOperation') {
        this.runtimeUpdateOperation = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async getRuntimeUpdateOperation(operationId: string) {
      const response = await sendRequest({
        type: 'getRuntimeUpdateOperation',
        payload: { operationId }
      })
      if (response.type === 'runtimeUpdateOperation') {
        this.runtimeUpdateOperation = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async cancelRuntimeDownload(operationId: string) {
      const response = await sendRequest({
        type: 'cancelRuntimeDownload',
        payload: { operationId }
      })
      if (response.type === 'runtimeUpdateOperation') {
        this.runtimeUpdateOperation = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async installDownloadedRuntime(operationId: string) {
      const response = await sendRequest({
        type: 'installDownloadedRuntime',
        payload: { operationId }
      })
      if (response.type === 'runtimeUpdateOperation') {
        this.runtimeUpdateOperation = response.payload
        if (response.payload.status === 'completed') {
          await this.loadPhpRuntimes()
        }
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async installPhpRuntime(artifactPath: string, releasePath: string) {
      const response = await sendRequest({
        type: 'installPhpRuntime',
        payload: { artifactPath, releasePath }
      })
      if (response.type === 'phpRuntimeInstalled') {
        this.phpRuntimes = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async setGlobalPhp(version: string) {
      const response = await sendRequest({ type: 'setGlobalPhp', payload: { version } })
      if (response.type === 'globalPhpChanged') {
        this.phpRuntimes = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async removePhpRuntime(version: string) {
      const response = await sendRequest({ type: 'removePhpRuntime', payload: { version } })
      if (response.type === 'phpRuntimeRemoved') {
        this.phpRuntimes = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async getPhpIni(phpVersion: string) {
      const response = await sendRequest({ type: 'getPhpIni', payload: { phpVersion } })
      if (response.type === 'phpIni') {
        return response.payload.contents
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async savePhpIni(phpVersion: string, contents: string) {
      const response = await sendRequest({
        type: 'savePhpIni',
        payload: { phpVersion, contents }
      })
      if (response.type === 'phpIniSaved') {
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async getDefaultPhpIni() {
      const response = await sendRequest({ type: 'getDefaultPhpIni' })
      if (response.type === 'defaultPhpIni') {
        return response.payload.contents
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async saveDefaultPhpIni(contents: string) {
      const response = await sendRequest({
        type: 'saveDefaultPhpIni',
        payload: { contents }
      })
      if (response.type === 'defaultPhpIniSaved') {
        return
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async getErpPhpIni(phpVersion: string | null) {
      const response = await sendRequest({
        type: 'getErpPhpIni',
        payload: { phpVersion }
      })
      if (response.type === 'erpPhpIni') {
        return response.payload.contents
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadNodeRuntime() {
      const response = await sendRequest({ type: 'getNodeRuntime' })
      if (response.type === 'nodeRuntime') {
        this.nodeRuntime = response.payload
        this.connected = true
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async installNodeRuntime(artifactPath: string, releasePath: string) {
      const response = await sendRequest({
        type: 'installNodeRuntime',
        payload: { artifactPath, releasePath }
      })
      if (response.type === 'nodeRuntimeInstalled') {
        this.nodeRuntime = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async removeNodeRuntime() {
      const response = await sendRequest({ type: 'removeNodeRuntime' })
      if (response.type === 'nodeRuntimeRemoved') {
        this.nodeRuntime = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadProxyManager() {
      const response = await sendRequest({ type: 'getProxyManager' })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        this.connected = true
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async addProxyConnection(input: ProxyConnectionInput) {
      const response = await sendRequest({ type: 'addProxyConnection', payload: input })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async updateProxyConnection(connectionId: string, input: ProxyConnectionInput) {
      const response = await sendRequest({
        type: 'updateProxyConnection',
        payload: { connectionId, input }
      })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async removeProxyConnection(connectionId: string) {
      const response = await sendRequest({
        type: 'removeProxyConnection',
        payload: { connectionId }
      })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async startProxyConnection(connectionId: string) {
      const response = await sendRequest({
        type: 'startProxyConnection',
        payload: { connectionId }
      })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async stopProxyConnection(connectionId: string) {
      const response = await sendRequest({
        type: 'stopProxyConnection',
        payload: { connectionId }
      })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async restartProxyConnection(connectionId: string) {
      await this.stopProxyConnection(connectionId)
      return this.startProxyConnection(connectionId)
    },
    async startAllProxyConnections() {
      const response = await sendRequest({ type: 'startAllProxyConnections' })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async stopAllProxyConnections() {
      const response = await sendRequest({ type: 'stopAllProxyConnections' })
      if (response.type === 'proxyManager') {
        this.proxyManager = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async revealPhpIni(phpVersion: string) {
      return invoke<string>('reveal_php_ini', { phpVersion })
    },
    async revealDefaultPhpIni() {
      return invoke<string>('reveal_default_php_ini')
    },
    async openSite(domain: string, secured: boolean) {
      return invoke<void>('open_site', { domain, secured })
    },
    async openProxyInChrome(domain: string, listenPort: number) {
      return invoke<void>('open_proxy_in_chrome', { domain, listenPort })
    },
    async addSite(input: SiteInput) {
      const response = await sendRequest({ type: 'addSite', payload: input })
      if (response.type === 'siteAdded') {
        this.sites.push(response.payload)
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async updateSite(siteId: string, input: SiteEditInput) {
      const response = await sendRequest({
        type: 'updateSite',
        payload: { siteId, input }
      })
      if (response.type === 'siteUpdated') {
        this.sites = this.sites.map((site) =>
          site.id === response.payload.id ? response.payload : site
        )
        if (this.lanShare) {
          this.lanShare.sites = this.lanShare.sites.map((site) =>
            site.siteId === response.payload.id
              ? { ...site, domain: response.payload.domain }
              : site
          )
        }
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async removeSite(siteId: string) {
      const response = await sendRequest({ type: 'removeSite', payload: { siteId } })
      if (response.type === 'siteRemoved') {
        this.sites = this.sites.filter((site) => site.id !== response.payload.id)
        if (this.lanShare) {
          this.lanShare.sites = this.lanShare.sites.filter(
            (site) => site.siteId !== response.payload.id
          )
          if (this.lanShare.sites.length === 0) {
            this.lanShare = null
          }
        }
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async setSitePhp(siteId: string, phpVersion: string | null) {
      const response = await sendRequest({
        type: 'setSitePhp',
        payload: { siteId, phpVersion }
      })
      if (response.type === 'sitePhpChanged') {
        this.sites = this.sites.map((site) =>
          site.id === response.payload.id ? response.payload : site
        )
        await this.loadPhpRuntimes()
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async setSiteHttps(siteId: string, secured: boolean) {
      if (secured) {
        const caResponse = await sendRequest({ type: 'ensureLocalCa' })
        if (caResponse.type === 'error') {
          throw new Error(caResponse.payload.message)
        }
        if (caResponse.type !== 'localCaReady') {
          throw new Error('Agent returned an unexpected response')
        }
        await invoke<void>('trust_local_ca', {
          certificatePath: caResponse.payload.certificatePath
        })
      }
      const response = await sendRequest({
        type: 'setSiteHttps',
        payload: { siteId, secured }
      })
      if (response.type === 'siteHttpsChanged') {
        this.sites = this.sites.map((site) =>
          site.id === response.payload.id ? response.payload : site
        )
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadLanShare() {
      const response = await sendRequest({ type: 'getLanShare' })
      if (response.type === 'lanShare') {
        this.lanShare = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async startLanShare(siteId: string, port = 18080) {
      const response = await sendRequest({
        type: 'startLanShare',
        payload: { siteId, port }
      })
      if (response.type === 'lanShare') {
        this.lanShare = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async stopLanShare() {
      const response = await sendRequest({ type: 'stopLanShare' })
      if (response.type === 'lanShare') {
        this.lanShare = response.payload
        return
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async stopLanShareSite(siteId: string) {
      const response = await sendRequest({ type: 'stopLanShareSite', payload: { siteId } })
      if (response.type === 'lanShare') {
        this.lanShare = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async startServicesOnLaunch() {
      this.busy = true
      this.error = null
      try {
        const statusResponse = await sendRequest({ type: 'getStatus' })
        if (statusResponse.type === 'error') {
          throw new Error(statusResponse.payload.message)
        }
        if (statusResponse.type !== 'status') {
          throw new Error('Agent returned an unexpected response')
        }

        this.status = statusResponse.payload
        this.connected = true
        const sitesResponse = await sendRequest({ type: 'listSites' })
        if (sitesResponse.type === 'error') {
          throw new Error(sitesResponse.payload.message)
        }
        if (sitesResponse.type !== 'sites') {
          throw new Error('Agent returned an unexpected response')
        }
        this.sites = sitesResponse.payload

        if (!hasEnabledSites(this.sites)) {
          const stopResponse = await sendRequest({ type: 'stopAll' })
          if (stopResponse.type === 'error') {
            throw new Error(stopResponse.payload.message)
          }
          if (stopResponse.type !== 'stopped') {
            throw new Error('Agent returned an unexpected response')
          }
          const stoppedStatus = await sendRequest({ type: 'getStatus' })
          if (stoppedStatus.type === 'error') {
            throw new Error(stoppedStatus.payload.message)
          }
          if (stoppedStatus.type !== 'status') {
            throw new Error('Agent returned an unexpected response')
          }
          this.status = stoppedStatus.payload
          return
        }

        if (areAllServicesRunning(statusResponse.payload)) {
          return
        }

        if (shouldStopServicesBeforeStart(statusResponse.payload)) {
          const stopResponse = await sendRequest({ type: 'stopAll' })
          if (stopResponse.type === 'error') {
            throw new Error(stopResponse.payload.message)
          }
          if (stopResponse.type !== 'stopped') {
            throw new Error('Agent returned an unexpected response')
          }
        }

        const startResponse = await sendRequest({ type: 'startAll' })
        if (startResponse.type === 'error') {
          throw new Error(startResponse.payload.message)
        }
        if (startResponse.type !== 'started') {
          throw new Error('Agent returned an unexpected response')
        }

        const finalStatus = await sendRequest({ type: 'getStatus' })
        if (finalStatus.type === 'error') {
          throw new Error(finalStatus.payload.message)
        }
        if (finalStatus.type !== 'status') {
          throw new Error('Agent returned an unexpected response')
        }
        this.status = finalStatus.payload
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async startAll() {
      this.busy = true
      this.error = null
      try {
        const response = await sendRequest({ type: 'startAll' })
        if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
        if (response.type !== 'started') {
          throw new Error('Agent returned an unexpected response')
        }
        await this.refreshStatus()
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async stopAll() {
      this.busy = true
      this.error = null
      try {
        const response = await sendRequest({ type: 'stopAll' })
        if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
        if (response.type !== 'stopped') {
          throw new Error('Agent returned an unexpected response')
        }
        this.lanShare = null
        await this.refreshStatus()
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async startMariaDb() {
      this.busy = true
      this.error = null
      try {
        const response = await sendRequest({ type: 'startMariaDb' })
        if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
        if (response.type !== 'mariaDbStarted') {
          throw new Error('Agent returned an unexpected response')
        }
        await this.refreshStatus()
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async stopMariaDb() {
      this.busy = true
      this.error = null
      try {
        const response = await sendRequest({ type: 'stopMariaDb' })
        if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
        if (response.type !== 'mariaDbStopped') {
          throw new Error('Agent returned an unexpected response')
        }
        await this.refreshStatus()
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      } finally {
        this.busy = false
      }
    },
    async restoreMariaDbOnLaunch() {
      try {
        const response = await sendRequest({ type: 'restoreMariaDbLastState' })
        if (response.type === 'error') {
          throw new Error(response.payload.message)
        }
        if (response.type !== 'mariaDbStateRestored') {
          throw new Error('Agent returned an unexpected response')
        }

        const statusResponse = await sendRequest({ type: 'getStatus' })
        if (statusResponse.type === 'error') {
          throw new Error(statusResponse.payload.message)
        }
        if (statusResponse.type !== 'status') {
          throw new Error('Agent returned an unexpected response')
        }
        this.status = statusResponse.payload
        this.connected = true
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error)
      }
    },
    async loadMariaDbSettings() {
      const response = await sendRequest({ type: 'getMariaDbSettings' })
      if (response.type === 'mariaDbSettings') {
        this.mariaDbSettings = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async saveMariaDbSettings(settings: MariaDbSettings) {
      const response = await sendRequest({ type: 'saveMariaDbSettings', payload: settings })
      if (response.type === 'mariaDbSettings') {
        this.mariaDbSettings = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async loadMariaDbConfig() {
      const response = await sendRequest({ type: 'getMariaDbConfig' })
      if (response.type === 'mariaDbConfig') {
        this.mariaDbConfig = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async saveMariaDbConfig(contents: string) {
      const response = await sendRequest({ type: 'saveMariaDbConfig', payload: { contents } })
      if (response.type === 'mariaDbConfigSaved') {
        this.mariaDbConfig = response.payload
        return response.payload
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async setMariaDbRootPassword(currentPassword: string, newPassword: string) {
      const response = await sendRequest({
        type: 'setMariaDbRootPassword',
        payload: { currentPassword, newPassword }
      })
      if (response.type === 'mariaDbRootPasswordChanged') {
        return
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async installMariaDbRuntime(artifactPath: string, releasePath: string) {
      const response = await sendRequest({
        type: 'installMariaDbRuntime',
        payload: { artifactPath, releasePath }
      })
      if (response.type === 'mariaDbRuntimeInstalled') {
        return response.payload.version
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    },
    async removeMariaDbRuntime() {
      const response = await sendRequest({ type: 'removeMariaDbRuntime' })
      if (response.type === 'mariaDbRuntimeRemoved') {
        return response.payload.version
      }
      if (response.type === 'error') {
        throw new Error(response.payload.message)
      }
      throw new Error('Agent returned an unexpected response')
    }
  }
})
