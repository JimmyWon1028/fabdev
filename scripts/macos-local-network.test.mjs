import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')

test('declares why fabDev connects to macOS local network Proxy targets', async () => {
  const desktopInfoPlist = await readFile(
    join(repoRoot, 'apps/desktop/src-tauri/Info.plist'),
    'utf8'
  )
  const agentInfoPlist = await readFile(
    join(repoRoot, 'crates/agent/Info.plist'),
    'utf8'
  )
  const agentBuildScript = await readFile(
    join(repoRoot, 'crates/agent/build.rs'),
    'utf8'
  )
  const runTauriScript = await readFile(
    join(repoRoot, 'scripts/run-tauri.sh'),
    'utf8'
  )
  const prepareDevAgentAppScript = await readFile(
    join(repoRoot, 'scripts/prepare-macos-dev-agent-app.sh'),
    'utf8'
  )

  assert.match(
    desktopInfoPlist,
    /<key>NSLocalNetworkUsageDescription<\/key>\s*<string>[^<]*Proxy targets[^<]*<\/string>/
  )
  assert.match(
    agentInfoPlist,
    /<key>NSLocalNetworkUsageDescription<\/key>\s*<string>[^<]*Proxy targets[^<]*<\/string>/
  )
  assert.match(agentInfoPlist, /<string>com\.fabdev\.agent<\/string>/)
  assert.match(agentInfoPlist, /<key>CFBundleExecutable<\/key>\s*<string>fabdev-agent<\/string>/)
  assert.match(agentBuildScript, /__TEXT,__info_plist/)
  assert.match(runTauriScript, /prepare-macos-dev-agent-app\.sh/)
  assert.match(prepareDevAgentAppScript, /codesign --force --deep --sign -/)
  assert.match(prepareDevAgentAppScript, /open -n --stdout/)
})
