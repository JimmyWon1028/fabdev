import { createRenderer, reactive, ssrContextKey, type Component } from 'vue'
import { beforeEach, expect, test, vi } from 'vitest'

import { ERP_MARIADB_CONFIG } from '../utils/mariadb'
import MariaDbView from './MariaDbView.vue'
import RuntimesView from './RuntimesView.vue'

const mocks = vi.hoisted(() => ({
  confirm: vi.fn(),
  leave: vi.fn(),
  store: {} as Record<string, unknown>,
  windows: false
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: mocks.confirm, open: vi.fn() }))
vi.mock('vue-router', () => ({ onBeforeRouteLeave: mocks.leave }))
vi.mock('../stores/fabdev', () => ({ useAppStore: () => mocks.store }))
vi.mock('../utils/path', () => ({
  isWindowsPlatform: () => mocks.windows,
  formatPathForDisplay: (path: string) => path
}))

interface HostNode {
  parent: HostNode | null
  children: HostNode[]
}

function node(): HostNode {
  return { parent: null, children: [] }
}

const renderer = createRenderer<HostNode, HostNode>({
  createElement: node,
  createText: node,
  createComment: node,
  setText() {},
  setElementText() {},
  patchProp() {},
  insert(child, parent, anchor = null) {
    child.parent = parent
    const index = anchor ? parent.children.indexOf(anchor) : -1
    parent.children.splice(index < 0 ? parent.children.length : index, 0, child)
  },
  remove(child) {
    const siblings = child.parent?.children
    if (siblings) {
      siblings.splice(siblings.indexOf(child), 1)
    }
    child.parent = null
  },
  parentNode: (child) => child.parent,
  nextSibling: (child) => {
    const siblings = child.parent?.children ?? []
    return siblings[siblings.indexOf(child) + 1] ?? null
  }
})

async function settle() {
  for (let index = 0; index < 12; index += 1) {
    await Promise.resolve()
  }
}

async function mount<T>(component: Component) {
  //2026.10.04 modify by codex, reason: exercise editor lifecycle without a browser or SSR template rendering.
  const app = renderer.createApp({ ...component, render: () => null })
  app.provide(ssrContextKey, {})
  const instance = app.mount(node())
  await settle()
  return {
    editor: (instance.$ as unknown as { setupState: T }).setupState,
    unmount: () => app.unmount()
  }
}

interface PhpEditor {
  phpIniSeries: string | null
  phpIniContents: string
  phpFpmRequestTimeoutSeconds: number
  phpFastCgiWorkers: number
  editPhpIni(runtime: { series: string }): Promise<void>
  closePhpIni(): Promise<void>
  savePhpIni(): Promise<void>
  savePhpFpmSettings(): Promise<void>
  savePhpFastCgiSettings(): Promise<void>
  loadErpPhpIni(): Promise<void>
}

interface MariaDbEditor {
  configContents: string
  port: string
  refreshPage(): Promise<void>
  loadSettings(): Promise<void>
  loadConfig(): Promise<void>
  saveConfig(): Promise<void>
  saveSettings(): Promise<void>
  applyErpConfig(): Promise<void>
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.windows = false
  mocks.confirm.mockResolvedValue(false)
  mocks.store = reactive({
    phpRuntimes: { installed: [], globalVersion: null },
    runtimeUpdateCheck: null,
    terminalPhp: null,
    status: { mariadb: 'stopped' },
    mariaDbSettings: { port: 3306 },
    busy: false,
    error: null,
    runtimeOperationFor: vi.fn(() => null),
    clearError: vi.fn(),
    setError: vi.fn(),
    refreshStatus: vi.fn().mockResolvedValue(undefined),
    loadPhpRuntimes: vi.fn().mockResolvedValue(undefined),
    loadTerminalPhp: vi.fn().mockResolvedValue(undefined),
    checkRuntimeUpdates: vi.fn().mockResolvedValue(undefined),
    getPhpIni: vi.fn().mockResolvedValue('memory_limit = 256M'),
    getPhpFpmSettings: vi.fn().mockResolvedValue({ requestTerminateTimeoutSeconds: 180 }),
    getPhpFastCgiSettings: vi.fn().mockResolvedValue({ workers: 4 }),
    getErpPhpIni: vi.fn().mockResolvedValue('memory_limit = 512M'),
    savePhpIni: vi.fn().mockResolvedValue(undefined),
    savePhpFpmSettings: vi.fn().mockResolvedValue({ requestTerminateTimeoutSeconds: 240 }),
    savePhpFastCgiSettings: vi.fn().mockResolvedValue({ workers: 8 }),
    loadMariaDbSettings: vi.fn().mockResolvedValue({ port: 3306, dataDir: '/test/data' }),
    loadMariaDbConfig: vi.fn().mockResolvedValue({ filename: 'my.cnf', contents: '[mysqld]\n' }),
    saveMariaDbConfig: vi.fn(async (contents: string) => ({ filename: 'my.cnf', contents })),
    saveMariaDbSettings: vi.fn(async (settings: unknown) => settings)
  })
})

test('PHP keeps a draft when switching versions or loading ERP parameters is cancelled', async () => {
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  editor.phpIniContents = 'memory_limit = 384M'
  await editor.editPhpIni({ series: '7.4' })
  expect(editor.phpIniSeries).toBe('8.2')
  expect(editor.phpIniContents).toBe('memory_limit = 384M')
  await editor.loadErpPhpIni()
  expect(editor.phpIniContents).toBe('memory_limit = 384M')
  expect(mocks.store.getErpPhpIni).not.toHaveBeenCalled()
  unmount()
})

test('PHP protects route changes and closing, and allows them after a successful save', async () => {
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  editor.phpIniContents = 'memory_limit = 384M'
  expect(mocks.leave).toHaveBeenCalledOnce()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  await editor.closePhpIni()
  expect(editor.phpIniSeries).toBe('8.2')
  await editor.savePhpIni()
  mocks.confirm.mockClear()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(true)
  await editor.closePhpIni()
  expect(editor.phpIniSeries).toBeNull()
  expect(mocks.confirm).not.toHaveBeenCalled()
  unmount()
})

test('PHP keeps failed saves dirty and only discards after explicit confirmation', async () => {
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  editor.phpIniContents = 'invalid configuration'
  vi.mocked(mocks.store.savePhpIni as () => Promise<void>).mockRejectedValue(new Error('invalid'))
  await editor.savePhpIni()
  await editor.editPhpIni({ series: '7.4' })
  expect(editor.phpIniSeries).toBe('8.2')
  mocks.confirm.mockResolvedValue(true)
  await editor.editPhpIni({ series: '7.4' })
  expect(editor.phpIniSeries).toBe('7.4')
  expect(editor.phpIniContents).toBe('memory_limit = 256M')
  unmount()
})

test.each([false, true])('PHP tracks unsaved platform settings (Windows: %s)', async (windows) => {
  mocks.windows = windows
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  if (windows) {
    editor.phpFastCgiWorkers = 8
  } else {
    editor.phpFpmRequestTimeoutSeconds = 240
  }
  editor.phpIniContents = 'memory_limit = 384M'
  expect(mocks.leave).toHaveBeenCalledOnce()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  if (windows) {
    await editor.savePhpFastCgiSettings()
  } else {
    await editor.savePhpFpmSettings()
  }
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  await editor.savePhpIni()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(true)
  unmount()
})

test('MariaDB protects edits from refresh, background reload, and ERP parameters', async () => {
  const { editor, unmount } = await mount<MariaDbEditor>(MariaDbView)
  editor.configContents = '[mysqld]\nmax_connections=250\n'
  editor.port = '3307'
  await editor.refreshPage()
  await editor.loadConfig()
  await editor.applyErpConfig()
  expect(editor.configContents).toBe('[mysqld]\nmax_connections=250\n')
  expect(editor.port).toBe('3307')
  expect(mocks.leave).toHaveBeenCalledOnce()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  unmount()
})

test('MariaDB only clears the dirty state of the settings that were saved', async () => {
  const { editor, unmount } = await mount<MariaDbEditor>(MariaDbView)
  editor.configContents = '[mysqld]\nmax_connections=250\n'
  editor.port = '3307'
  await editor.saveConfig()
  expect(mocks.leave).toHaveBeenCalledOnce()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  await editor.saveSettings()
  mocks.confirm.mockClear()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(true)
  expect(mocks.confirm).not.toHaveBeenCalled()
  unmount()
})

test('MariaDB reloads saved values after an explicitly confirmed refresh', async () => {
  const { editor, unmount } = await mount<MariaDbEditor>(MariaDbView)
  editor.configContents = 'draft'
  editor.port = '3307'
  mocks.confirm.mockResolvedValue(true)
  await editor.refreshPage()
  expect(editor.configContents).toBe('[mysqld]\n')
  expect(editor.port).toBe('3306')
  expect(mocks.leave).toHaveBeenCalledOnce()
  mocks.confirm.mockClear()
  expect(await mocks.leave.mock.calls[0][0]()).toBe(true)
  expect(mocks.confirm).not.toHaveBeenCalled()
  unmount()
})

test('PHP blocks closing and navigation while saving and marks only the submitted content saved', async () => {
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  editor.phpIniContents = 'memory_limit = 384M'
  let complete!: () => void
  vi.mocked(mocks.store.savePhpIni as () => Promise<void>).mockImplementation(() =>
    new Promise<void>((resolve) => { complete = resolve })
  )
  const saving = editor.savePhpIni()
  await editor.closePhpIni()
  expect(editor.phpIniSeries).toBe('8.2')
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  expect(mocks.confirm).not.toHaveBeenCalled()
  complete()
  await saving
  expect(await mocks.leave.mock.calls[0][0]()).toBe(true)
  unmount()
})

test('MariaDB preserves edits made while background reads are in flight', async () => {
  const { editor, unmount } = await mount<MariaDbEditor>(MariaDbView)
  let completeSettings!: (value: unknown) => void
  let completeConfig!: (value: unknown) => void
  vi.mocked(mocks.store.loadMariaDbSettings as () => Promise<unknown>).mockImplementation(() =>
    new Promise((resolve) => { completeSettings = resolve })
  )
  vi.mocked(mocks.store.loadMariaDbConfig as () => Promise<unknown>).mockImplementation(() =>
    new Promise((resolve) => { completeConfig = resolve })
  )
  const settings = editor.loadSettings()
  const config = editor.loadConfig()
  editor.port = '3308'
  editor.configContents = 'new draft'
  completeSettings({ port: 3306, dataDir: '/test/data' })
  completeConfig({ filename: 'my.cnf', contents: '[mysqld]\n' })
  await Promise.all([settings, config])
  expect(editor.port).toBe('3308')
  expect(editor.configContents).toBe('new draft')
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  unmount()
})

test('MariaDB retains failed saves and requires confirmation before replacing the draft', async () => {
  const { editor, unmount } = await mount<MariaDbEditor>(MariaDbView)
  editor.configContents = 'invalid configuration'
  vi.mocked(mocks.store.saveMariaDbConfig as () => Promise<unknown>).mockRejectedValue(new Error('invalid'))
  await editor.saveConfig()
  expect(editor.configContents).toBe('invalid configuration')
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  mocks.confirm.mockResolvedValue(true)
  await editor.applyErpConfig()
  expect(editor.configContents).toBe(ERP_MARIADB_CONFIG)
  mocks.confirm.mockResolvedValue(false)
  expect(await mocks.leave.mock.calls[0][0]()).toBe(false)
  unmount()
})

test('PHP ignores a second discard action while its confirmation is open', async () => {
  const { editor, unmount } = await mount<PhpEditor>(RuntimesView)
  await editor.editPhpIni({ series: '8.2' })
  editor.phpIniContents = 'draft'
  let complete!: (discard: boolean) => void
  mocks.confirm.mockImplementation(() => new Promise<boolean>((resolve) => { complete = resolve }))
  const closing = editor.closePhpIni()
  await editor.editPhpIni({ series: '7.4' })
  expect(mocks.confirm).toHaveBeenCalledOnce()
  expect(editor.phpIniSeries).toBe('8.2')
  complete(false)
  await closing
  expect(editor.phpIniContents).toBe('draft')
  unmount()
})
