import assert from 'node:assert/strict'
import { execFile } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

import { prepareAppRelease } from './generate-app-release-manifest.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const projectVersion = JSON.parse(await readFile(join(repoRoot, 'package.json'), 'utf8')).version
const execFileAsync = promisify(execFile)

function digest(contents) {
  return createHash('sha256').update(contents).digest('hex')
}

async function createRuntimePackageDirectory(testRoot, packages) {
  const directory = join(testRoot, 'runtime-packages')
  await mkdir(directory)
  for (const [fileName, contents] of Object.entries(packages)) {
    await writeFile(join(directory, fileName), contents)
  }
  return directory
}

test('blocks Windows installation until the x64 Visual C++ Runtime is complete', async () => {
  const hooks = await readFile(
    join(repoRoot, 'apps/desktop/src-tauri/windows/installer-hooks.nsh'),
    'utf8'
  )

  assert.match(hooks, /!macro NSIS_HOOK_PREINSTALL/)
  assert.match(hooks, /VC\\Runtimes\\x64" "Installed"/)
  assert.match(hooks, /\$WINDIR\\Sysnative\\VCRUNTIME140\.dll/)
  assert.match(hooks, /\$WINDIR\\System32\\VCRUNTIME140\.dll/)
  assert.match(hooks, /https:\/\/aka\.ms\/vc14\/vc_redist\.x64\.exe/)
  assert.match(hooks, /\$\(FabDevVcRuntimeRequired\)/)
  assert.match(hooks, /\$\{Silent\}/)
  assert.match(hooks, /ExecShell "open"/)

  const preinstall = hooks.indexOf('!macro NSIS_HOOK_PREINSTALL')
  const preuninstall = hooks.indexOf('!macro NSIS_HOOK_PREUNINSTALL')
  assert.ok(preinstall >= 0 && preinstall < preuninstall)
})

test('prepares canonical release assets, checksums, and manifests', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))

  const macContents = Buffer.from('macOS installer fixture')
  const windowsContents = Buffer.from('Windows installer fixture')
  const connectContents = Buffer.from('fabDev Connect fixture')
  const macRuntimeContents = Buffer.from('macOS PHP Runtime fixture')
  const windowsRuntimeContents = Buffer.from('Windows PHP Runtime fixture')
  const macSource = join(testRoot, 'input.dmg')
  const windowsSource = join(testRoot, 'input-setup.exe')
  const connectSource = join(testRoot, 'input-connect.exe')
  const outputDir = join(testRoot, 'release')
  await writeFile(macSource, macContents)
  await writeFile(windowsSource, windowsContents)
  await writeFile(connectSource, connectContents)
  const runtimePackageDir = await createRuntimePackageDirectory(testRoot, {
    'php-8.4.24-macos-arm64-community.tar.gz': macRuntimeContents,
    'php-8.4.24-windows-x64-community.tar.gz': windowsRuntimeContents
  })

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-28T12:34:56Z',
    outputDir,
    macosArm64: macSource,
    windowsX64: windowsSource,
    windowsConnectX64: connectSource,
    runtimePackageDirs: [runtimePackageDir]
  })

  const macName = `fabDev-Community-${projectVersion}-macos-arm64.dmg`
  const windowsName = `fabDev-Community-${projectVersion}-windows-x64-setup.exe`
  const connectName = `fabDev-Connect-${projectVersion}-windows-x64.exe`
  const macRuntimeName = 'php-8.4.24-macos-arm64-community.tar.gz'
  const windowsRuntimeName = 'php-8.4.24-windows-x64-community.tar.gz'
  assert.deepEqual((await readdir(outputDir)).sort(), [
    'SHA256SUMS',
    'fabdev-app-v1.json',
    'fabdev-stable-v1.json',
    connectName,
    `${connectName}.sha256`,
    macName,
    `${macName}.sha256`,
    macRuntimeName,
    `${macRuntimeName}.sha256`,
    windowsName,
    `${windowsName}.sha256`,
    windowsRuntimeName,
    `${windowsRuntimeName}.sha256`
  ].sort())

  assert.equal(await readFile(join(outputDir, macName), 'utf8'), macContents.toString())
  assert.equal(await readFile(join(outputDir, windowsName), 'utf8'), windowsContents.toString())
  assert.equal(await readFile(join(outputDir, connectName), 'utf8'), connectContents.toString())
  assert.equal(
    await readFile(join(outputDir, macRuntimeName), 'utf8'),
    macRuntimeContents.toString()
  )
  assert.equal(
    await readFile(join(outputDir, windowsRuntimeName), 'utf8'),
    windowsRuntimeContents.toString()
  )

  const manifest = JSON.parse(await readFile(join(outputDir, 'fabdev-app-v1.json'), 'utf8'))
  const stableManifest = JSON.parse(
    await readFile(join(outputDir, 'fabdev-stable-v1.json'), 'utf8')
  )
  assert.deepEqual(stableManifest, manifest)
  assert.equal(manifest.version, projectVersion)
  assert.equal(manifest.tag, `v${projectVersion}`)
  assert.equal(manifest.publishedAt, '2026-08-28T12:34:56Z')
  assert.equal(manifest.compatibility.agentProtocolVersion, 36)
  assert.equal(manifest.artifacts.length, 2)
  assert.equal(manifest.artifacts[0].fileName, macName)
  assert.equal(manifest.artifacts[0].sha256, digest(macContents))
  assert.equal(manifest.artifacts[0].signature, null)
  assert.equal(manifest.artifacts[1].fileName, windowsName)
  assert.equal(manifest.artifacts[1].sha256, digest(windowsContents))
  assert.equal(manifest.artifacts.some((artifact) => artifact.fileName === connectName), false)

  assert.equal(
    await readFile(join(outputDir, 'SHA256SUMS'), 'utf8'),
    `${digest(macContents)}  ${macName}\n` +
      `${digest(windowsContents)}  ${windowsName}\n` +
      `${digest(connectContents)}  ${connectName}\n` +
      `${digest(macRuntimeContents)}  ${macRuntimeName}\n` +
      `${digest(windowsRuntimeContents)}  ${windowsRuntimeName}\n`
  )
  assert.equal(
    await readFile(join(outputDir, `${macName}.sha256`), 'utf8'),
    `${digest(macContents)}  ${macName}\n`
  )
  assert.equal(result.outputDir, outputDir)
})

test('rejects a release version that differs from project metadata', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const source = join(testRoot, 'input.dmg')
  await writeFile(source, 'fixture')

  await assert.rejects(
    prepareAppRelease({
      repoRoot,
      version: '9.9.9',
      publishedAt: '2026-08-28T12:34:56Z',
      outputDir: join(testRoot, 'release'),
      macosArm64: source
    }),
    /does not match project version/
  )
})

test('prepares a Windows-only release with a Windows-only Runtime package', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-windows-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const windowsSource = join(testRoot, 'input-setup.exe')
  const connectSource = join(testRoot, 'input-connect.exe')
  const outputDir = join(testRoot, 'release')
  await writeFile(windowsSource, 'Windows installer fixture')
  await writeFile(connectSource, 'fabDev Connect fixture')
  const runtimePackageDir = await createRuntimePackageDirectory(testRoot, {
    'php-7.4.33-windows-x64-community.tar.gz': 'Windows PHP 7.4 Runtime fixture',
    'php-8.2.33-windows-x64-community.tar.gz': 'Windows PHP 8.2 Runtime fixture',
    'php-8.4.24-windows-x64-community.tar.gz': 'Windows PHP 8.4 Runtime fixture',
    'mariadb-12.3.2-windows-x64-community.tar.gz': 'Windows MariaDB Runtime fixture',
    'node-20.20.2-windows-x64-community.tar.gz': 'Windows Node.js 20 Runtime fixture',
    'node-24.20.0-windows-x64-community.tar.gz': 'Windows Node.js 24 Runtime fixture'
  })

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-30T12:34:56Z',
    outputDir,
    windowsX64: windowsSource,
    windowsConnectX64: connectSource,
    runtimePackageDirs: [runtimePackageDir]
  })

  assert.equal(result.manifest.artifacts.length, 1)
  assert.equal(result.manifest.artifacts[0].platform, 'windows')
  assert.equal(result.files.length, 19)
  assert.equal(
    result.files.includes('php-7.4.33-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.includes('php-8.2.33-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.includes('php-8.4.24-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.includes('mariadb-12.3.2-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.includes('node-20.20.2-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.includes('node-24.20.0-windows-x64-community.tar.gz'),
    true
  )
  assert.equal(
    result.files.some((fileName) => fileName.includes('macos')),
    false
  )
})

test('stages future Runtime versions without adding version-specific options', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-future-runtime-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const windowsSource = join(testRoot, 'input-setup.exe')
  const outputDir = join(testRoot, 'release')
  await writeFile(windowsSource, 'Windows installer fixture')
  const runtimePackageDir = await createRuntimePackageDirectory(testRoot, {
    'php-8.5.1-windows-x64-community.tar.gz': 'Future PHP Runtime fixture',
    'node-26.1.3-windows-x64-community.tar.gz': 'Future Node.js Runtime fixture',
    'notes.txt': 'Ignored non-package file'
  })

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-09-01T12:34:56Z',
    outputDir,
    windowsX64: windowsSource,
    runtimePackageDirs: [runtimePackageDir]
  })

  assert.equal(result.files.includes('php-8.5.1-windows-x64-community.tar.gz'), true)
  assert.equal(result.files.includes('node-26.1.3-windows-x64-community.tar.gz'), true)
  assert.equal(result.files.includes('notes.txt'), false)
})

test('prepares a macOS ARM64 release with all online Runtime packages', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-macos-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const macSource = join(testRoot, 'input.dmg')
  const outputDir = join(testRoot, 'release')
  await writeFile(macSource, 'macOS installer fixture')
  const runtimePackageDir = await createRuntimePackageDirectory(testRoot, {
    'php-8.4.24-macos-arm64-community.tar.gz': 'macOS PHP Runtime fixture',
    'mariadb-12.3.2-macos-arm64-community.tar.gz': 'macOS MariaDB Runtime fixture',
    'node-20.20.2-macos-arm64-community.tar.gz': 'macOS Node.js 20 Runtime fixture',
    'node-24.20.0-macos-arm64-community.tar.gz': 'macOS Node.js 24 Runtime fixture'
  })

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-31T06:00:00Z',
    outputDir,
    macosArm64: macSource,
    runtimePackageDirs: [runtimePackageDir]
  })

  assert.equal(result.manifest.artifacts.length, 1)
  assert.equal(result.manifest.artifacts[0].platform, 'macos')
  assert.equal(result.manifest.artifacts[0].architecture, 'arm64')
  assert.equal(result.files.length, 13)
  assert.equal(result.files.includes('php-8.4.24-macos-arm64-community.tar.gz'), true)
  assert.equal(result.files.includes('mariadb-12.3.2-macos-arm64-community.tar.gz'), true)
  assert.equal(result.files.includes('node-20.20.2-macos-arm64-community.tar.gz'), true)
  assert.equal(result.files.includes('node-24.20.0-macos-arm64-community.tar.gz'), true)
  assert.equal(result.files.some((fileName) => fileName.includes('windows')), false)
})

test('refuses to overwrite an existing output directory', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const source = join(testRoot, 'input.dmg')
  const outputDir = join(testRoot, 'release')
  await writeFile(source, 'fixture')
  await mkdir(outputDir)

  await assert.rejects(
    prepareAppRelease({
      repoRoot,
      version: projectVersion,
      publishedAt: '2026-08-28T12:34:56Z',
      outputDir,
      macosArm64: source
    }),
    /Output directory already exists/
  )
})

test('keeps the Draft Release workflow manual and unable to publish', async () => {
  const workflow = (
    await readFile(join(repoRoot, '.github/workflows/release-draft.yml'), 'utf8')
  ).replaceAll('\r\n', '\n')
  const triggerStart = workflow.indexOf('\non:\n')
  const permissionsStart = workflow.indexOf('\npermissions:\n')
  const createDraftStart = workflow.indexOf('\n  create-draft:\n')

  assert.notEqual(triggerStart, -1)
  assert.notEqual(permissionsStart, -1)
  assert.notEqual(createDraftStart, -1)

  const triggerBlock = workflow.slice(triggerStart, permissionsStart)
  assert.match(triggerBlock, /\n  workflow_dispatch:\n/)
  assert.doesNotMatch(triggerBlock, /\n  (push|pull_request|schedule|release):/)

  assert.match(workflow, /CONFIRM_REPACKAGE: \$\{\{ inputs\.confirm_repackage \}\}/)
  assert.match(workflow, /CONFIRM_DRAFT: \$\{\{ inputs\.confirm_draft \}\}/)
  assert.match(workflow, /RUNTIME_CATALOG_SEQUENCE: \$\{\{ inputs\.runtime_catalog_sequence \}\}/)
  assert.match(workflow, /RUNTIME_CATALOG_EXPIRES_AT: \$\{\{ inputs\.runtime_catalog_expires_at \}\}/)
  assert.match(workflow, /REPACKAGE v\$VERSION/)
  assert.match(workflow, /DRAFT v\$VERSION/)
  assert.match(workflow, /permissions:\n  contents: read/)
  assert.doesNotMatch(workflow.slice(0, createDraftStart), /contents: write/)
  assert.match(workflow.slice(createDraftStart), /permissions:\n      contents: write/)
  assert.match(workflow, /gh release create/)
  assert.match(workflow, /--draft/)
  assert.match(workflow, /--verify-tag/)
  assert.match(workflow, /--latest=false/)
  assert.match(workflow, /releases\?per_page=100/)
  assert.doesNotMatch(workflow, /releases\/tags\//)
  assert.doesNotMatch(workflow, /gh release edit|--draft=false|make_latest/)
  assert.doesNotMatch(workflow, /secrets\./)
  assert.match(workflow, /build-windows-php-runtime\.ps1/)
  assert.match(workflow, /-OutputDirectory release-input/)
  assert.match(workflow, /-ManifestPath resources\/runtime-packages\/windows-x64\.json/)
  assert.match(workflow, /prepare-windows-runtimes\.ps1/)
  assert.match(
    workflow,
    /-ManifestPath resources\/runtime-packages\/windows-x64-bundled\.json/
  )
  assert.match(workflow, /FABDEV_WINDOWS_RUNTIME_PACKAGE_MANIFEST:/)
  assert.match(workflow, /FABDEV_WINDOWS_RUNTIME_PACKAGE_DIR:/)
  assert.doesNotMatch(workflow, /FABDEV_WINDOWS_PHP(?:74|82|84)_RUNTIME_PACKAGE:/)
  assert.match(workflow, /installs_real_windows_php_archive/)
  assert.match(workflow, /FABDEV_WINDOWS_RUNTIME_NAMES: mariadb node/)
  assert.match(workflow, /FABDEV_WINDOWS_PACKAGE_MANIFEST:/)
  assert.match(workflow, /--runtime-package-dir/)
  assert.doesNotMatch(workflow, /--runtime-(?:php|mariadb|node\d|windows|macos)/)
  assert.match(workflow, /build-macos:/)
  assert.match(workflow, /draft-macos-arm64/)
  assert.match(workflow, /draft-macos-online-runtimes/)
  assert.match(workflow, /build-macos-online-runtime-packages\.sh/)
  assert.match(workflow, /--macos-arm64/)
  assert.match(workflow, /generate-community/)
  assert.match(workflow, /--bin fabdev-runtime-catalog/)
  assert.match(workflow, /release-assets\/fabdev-runtime-v1\.json/)
  assert.match(workflow, /expected_file_count="\$\(\(checksum_entries \* 2 \+ 4\)\)"/)

  const usesLines = workflow
    .split('\n')
    .filter((line) => /^\s+uses:/.test(line))
  assert.ok(usesLines.length > 0)
  for (const line of usesLines) {
    assert.match(line, /^\s+uses: [^@\s]+@[0-9a-f]{40}(?:\s+#.*)?$/)
  }
})

test('launches the Windows updater only after fabDev and its Agent exit', async () => {
  const desktopSource = await readFile(
    join(repoRoot, 'apps/desktop/src-tauri/src/lib.rs'),
    'utf8'
  )

  assert.match(desktopSource, /Wait-Process -Id \$ParentProcessId/)
  assert.match(desktopSource, /Get-FabDevAgentProcesses/)
  assert.match(desktopSource, /\[System\.StringComparison\]::OrdinalIgnoreCase/)
  assert.match(desktopSource, /Stop-Process -Id \$agentIds -Force -ErrorAction SilentlyContinue/)
  assert.match(desktopSource, /throw 'fabDev Agent did not exit before the update'/)
  assert.match(desktopSource, /Test-FabDevAgentFileUnlocked/)
  assert.match(desktopSource, /\[System\.IO\.FileShare\]::None/)
  assert.match(desktopSource, /throw 'fabDev Agent executable is still locked before the update'/)
  assert.match(desktopSource, /Start-Process -FilePath \$InstallerPath/)
  assert.match(desktopSource, /-ArgumentList @\('\/UPDATE', '\/P', '\/R'\)/)
  assert.match(desktopSource, /Set-Content -LiteralPath \$ReadyPath/)
  assert.match(desktopSource, /WINDOWS_UPDATE_LAUNCHER_READY_TIMEOUT/)
  assert.match(desktopSource, /"-File",/)
  assert.match(desktopSource, /\.arg\("-AgentPath"\)/)
  assert.match(desktopSource, /\.arg\(std::process::id\(\)\.to_string\(\)\)/)
})

test('keeps recoverable Windows startup errors visible and logged', async () => {
  const [desktopSource, appSource] = await Promise.all([
    readFile(join(repoRoot, 'apps/desktop/src-tauri/src/lib.rs'), 'utf8'),
    readFile(join(repoRoot, 'apps/desktop/src/App.vue'), 'utf8')
  ])

  assert.match(desktopSource, /const DESKTOP_PROCESS_LOG_FILE: &str = "desktop-process\.log"/)
  assert.match(desktopSource, /install_desktop_panic_logging\(\)/)
  assert.match(desktopSource, /if let Err\(error\) = install_bundled_windows_runtimes\(app\)/)
  assert.match(desktopSource, /if let Err\(error\) = setup_tray\(app\)/)
  assert.match(desktopSource, /app_handle\.emit\(AGENT_ERROR_EVENT, startup_errors\.join\("\\n"\)\)/)
  assert.match(desktopSource, /fn record_desktop_error\(source: String, message: String\)/)
  assert.match(appSource, /window\.addEventListener\('error', handleWindowError\)/)
  assert.match(appSource, /window\.addEventListener\('unhandledrejection', handleUnhandledRejection\)/)
})

test('wires Windows update download cancellation through the desktop command', async () => {
  const [desktopSource, updaterSource, settingsSource] = await Promise.all([
    readFile(join(repoRoot, 'apps/desktop/src-tauri/src/lib.rs'), 'utf8'),
    readFile(join(repoRoot, 'crates/updater/src/lib.rs'), 'utf8'),
    readFile(join(repoRoot, 'apps/desktop/src/views/SettingsView.vue'), 'utf8')
  ])

  assert.match(desktopSource, /fn cancel_app_update_download\(\)/)
  assert.match(desktopSource, /APP_UPDATE_DOWNLOAD_CANCEL_REQUESTED\.store\(true/)
  assert.match(desktopSource, /download_app_update_with_cancellation/)
  assert.match(updaterSource, /&is_cancelled/)
  assert.match(settingsSource, /store\.cancelAppUpdateDownload\(\)/)
  assert.match(settingsSource, /store\.appUpdateDownloading && canCancelAppUpdate/)
})

test('loads and verifies every Windows online PHP Runtime from the package manifest', async () => {
  const [script, manifestContents] = await Promise.all([
    readFile(join(repoRoot, 'scripts/build-windows-php-runtime.ps1'), 'utf8'),
    readFile(join(repoRoot, 'resources/runtime-packages/windows-x64.json'), 'utf8')
  ])
  const manifest = JSON.parse(manifestContents)
  const phpPackages = manifest.packages.filter((runtimePackage) => runtimePackage.name === 'php')

  assert.deepEqual(phpPackages.map((runtimePackage) => runtimePackage.version), [
    '7.4.33',
    '8.2.33',
    '8.4.24'
  ])
  assert.equal(
    phpPackages.every(
      (runtimePackage) =>
        runtimePackage.source.verification.method === 'official-sha256' &&
        /^[0-9a-f]{64}$/.test(runtimePackage.source.archiveSha256)
    ),
    true
  )
  assert.match(script, /ConvertFrom-Json/)
  assert.match(script, /Where-Object \{ \$_\.name -eq "php" \}/)
  assert.match(script, /php-\$phpVersion-windows-x64-community\.tar\.gz/)
  assert.match(script, /ext\/php_mysqli\.dll/)
  assert.match(script, /ext\/php_pdo_mysql\.dll/)
  assert.match(script, /extension_loaded\('mysqli'\)/)
  assert.match(script, /extension_loaded\('pdo_mysql'\)/)
  assert.match(script, /tar\.exe -tzf/)
  assert.match(script, /foreach \(\$runtime in \$phpRuntimes\)/)
  assert.doesNotMatch(script, /7\.4\.33|8\.2\.33|8\.4\.24/)
  assert.doesNotMatch(script, /mariadb|node/i)
})

test('loads bundled Windows Runtime versions and the default PHP from a manifest', async () => {
  const [script, desktopSource, manifestContents] = await Promise.all([
    readFile(join(repoRoot, 'scripts/prepare-windows-runtimes.ps1'), 'utf8'),
    readFile(join(repoRoot, 'apps/desktop/src-tauri/src/lib.rs'), 'utf8'),
    readFile(
      join(repoRoot, 'resources/runtime-packages/windows-x64-bundled.json'),
      'utf8'
    )
  ])
  const manifest = JSON.parse(manifestContents)
  const defaultPhp = manifest.packages.filter(
    (runtimePackage) => runtimePackage.name === 'php' && runtimePackage.default
  )

  assert.equal(defaultPhp.length, 1)
  assert.match(script, /ConvertFrom-Json/)
  assert.match(script, /defaultPhpVersion/)
  assert.doesNotMatch(script, /7\.4\.33|8\.2\.33|1\.30\.4/)
  assert.match(desktopSource, /struct BundledWindowsRuntimeManifest/)
  assert.match(desktopSource, /default_php_version/)
  const windowsInstallerStart = desktopSource.indexOf('fn install_bundled_windows_runtimes')
  const windowsInstallerEnd = desktopSource.indexOf(
    '#[cfg(any(target_os = "macos", windows))]',
    windowsInstallerStart
  )
  assert.ok(windowsInstallerStart >= 0 && windowsInstallerEnd > windowsInstallerStart)
  assert.doesNotMatch(
    desktopSource.slice(windowsInstallerStart, windowsInstallerEnd),
    /version\.starts_with\("8\.2\."\)/
  )
})

test('prepares bundled macOS Runtimes and their default PHP from a manifest', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-bundled-macos-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const sourceRoot = join(testRoot, 'source')
  const outputRoot = join(testRoot, 'output')
  const manifestPath = join(repoRoot, 'resources/runtime-packages/macos-arm64-bundled.json')
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'))
  await mkdir(sourceRoot)

  for (const runtimePackage of manifest.packages) {
    const contents = Buffer.from(`${runtimePackage.name} ${runtimePackage.version}`)
    const stem = `${runtimePackage.name}-${runtimePackage.version}-macos-arm64-dev`
    await writeFile(join(sourceRoot, `${stem}.tar.gz`), contents)
    await writeFile(
      join(sourceRoot, `${stem}.json`),
      JSON.stringify({
        name: runtimePackage.name,
        version: runtimePackage.version,
        platform: 'macos',
        architecture: 'arm64',
        size: contents.length,
        sha256: digest(contents),
        url: `${stem}.tar.gz`
      })
    )
  }

  await execFileAsync(process.execPath, [join(repoRoot, 'scripts/prepare-bundled-runtime-assets.mjs')], {
    env: {
      ...process.env,
      FABDEV_BUNDLED_RUNTIME_SOURCE: sourceRoot,
      FABDEV_BUNDLED_RUNTIME_OUTPUT: outputRoot,
      FABDEV_BUNDLED_RUNTIME_MANIFEST: manifestPath
    }
  })

  const bundledManifest = JSON.parse(await readFile(join(outputRoot, 'manifest.json'), 'utf8'))
  assert.deepEqual(
    bundledManifest.packages.map(({ name, version, default: isDefault }) => [
      name,
      version,
      isDefault
    ]),
    manifest.packages.map(({ name, version, default: isDefault = false }) => [
      name,
      version,
      isDefault
    ])
  )
  assert.equal(
    bundledManifest.packages.filter(
      (runtimePackage) => runtimePackage.name === 'php' && runtimePackage.default
    ).length,
    1
  )
})

test('pins and prepares every macOS ARM64 online Runtime package', async () => {
  const manifest = JSON.parse(
    await readFile(join(repoRoot, 'resources/runtime-packages/macos-arm64.json'), 'utf8')
  )
  const bundledManifest = JSON.parse(
    await readFile(
      join(repoRoot, 'resources/runtime-packages/macos-arm64-bundled.json'),
      'utf8'
    )
  )
  const packageBuild = await readFile(
    join(repoRoot, 'scripts/build-macos-runtime-packages.sh'),
    'utf8'
  )
  const nodeBuild = await readFile(join(repoRoot, 'scripts/build-node-runtime.sh'), 'utf8')
  const phpBuild = await readFile(join(repoRoot, 'scripts/build-php-runtime.sh'), 'utf8')
  const mariaDbBuild = await readFile(
    join(repoRoot, 'scripts/build-mariadb-runtime.sh'),
    'utf8'
  )
  const releaseBuild = await readFile(
    join(repoRoot, 'scripts/build-macos-online-runtime-packages.sh'),
    'utf8'
  )
  const phpPackage = await readFile(join(repoRoot, 'scripts/package-php-runtime.sh'), 'utf8')
  const phpHealthCheck = await readFile(
    join(repoRoot, 'scripts/validate-php-runtime-health.sh'),
    'utf8'
  )
  const phpSiteHealthCheck = await readFile(
    join(repoRoot, 'scripts/validate-php-runtime-site.sh'),
    'utf8'
  )
  const phpOnlineFlowCheck = await readFile(
    join(repoRoot, 'scripts/validate-macos-php-online-flow.sh'),
    'utf8'
  )
  const mariaDbPackage = await readFile(
    join(repoRoot, 'scripts/package-mariadb-runtime.sh'),
    'utf8'
  )
  const mariaDbHealthCheck = await readFile(
    join(repoRoot, 'scripts/validate-mariadb-runtime-health.sh'),
    'utf8'
  )
  const nodePackage = await readFile(join(repoRoot, 'scripts/package-node-runtime.sh'), 'utf8')
  const minimumVersionCheck = await readFile(
    join(repoRoot, 'scripts/validate-macos-runtime-minimum.sh'),
    'utf8'
  )
  const dependencyBuild = await readFile(
    join(repoRoot, 'scripts/build-macos-runtime-dependencies.sh'),
    'utf8'
  )
  const dylibBundle = await readFile(
    join(repoRoot, 'scripts/bundle-macos-dylibs.sh'),
    'utf8'
  )

  assert.deepEqual(
    manifest.packages.map((runtimePackage) => [runtimePackage.name, runtimePackage.version]),
    [
      ['php', '8.4.24'],
      ['mariadb', '12.3.2'],
      ['node', '20.20.2'],
      ['node', '24.20.0']
    ]
  )
  assert.deepEqual(
    bundledManifest.packages.map((runtimePackage) => [
      runtimePackage.name,
      runtimePackage.version
    ]),
    [
      ['dnsmasq', '2.93'],
      ['nginx', '1.30.4'],
      ['php', '7.4.33'],
      ['php', '8.2.33']
    ]
  )
  assert.match(packageBuild, /jq -c '\.packages\[\]'/)
  assert.match(packageBuild, /version="\$\(jq -r '\.version'/)
  assert.match(packageBuild, /build_profile="\$\(jq -r '\.buildProfile/)
  assert.match(packageBuild, /NODE_VERSION="\$version"/)
  assert.match(packageBuild, /PHP_VERSION="\$version"/)
  assert.doesNotMatch(
    packageBuild,
    /7\.4\.33|8\.2\.33|8\.4\.24|12\.3\.2|20\.20\.2|24\.20\.0/
  )

  assert.match(nodeBuild, /20\.20\.2\)/)
  assert.match(nodeBuild, /466e05f3477c20dfb723054dfebffe55bc74660ee77f612166fca121dacb65b6/)
  assert.match(nodeBuild, /24\.20\.0\)/)
  assert.match(nodeBuild, /40e5607e5ecb3db9192723776da2d75d966260fc74a7a9e731c1bd67dda96bc8/)
  assert.match(phpBuild, /MACOS_TARGET="\$\{MACOSX_DEPLOYMENT_TARGET:-13\.0\}"/)
  assert.match(phpBuild, /build-macos-runtime-dependencies\.sh" php/)
  assert.match(phpBuild, /FABDEV_RUNTIME_DEPENDENCY_PREFIX/)
  assert.match(phpBuild, /--with-iconv="\$DEPENDENCY_PREFIX"/)
  assert.match(mariaDbBuild, /MACOS_TARGET="\$\{MACOSX_DEPLOYMENT_TARGET:-13\.0\}"/)
  assert.match(mariaDbBuild, /OPENSSL_USE_STATIC_LIBS=TRUE/)
  assert.match(mariaDbBuild, /PLUGIN_MROONGA=NO/)
  assert.match(mariaDbBuild, /CLIENT_PLUGIN_ZSTD=OFF/)
  assert.match(dependencyBuild, /OPENSSL_VERSION="3\.6\.3"/)
  assert.match(dependencyBuild, /--openssldir=\/etc\/ssl/)
  assert.match(dependencyBuild, /243a86649cf6f23eeb6a2ff2456e09e5d77dd9018a54d3d96b0c6bdd6ba6c7f1/)
  assert.match(dependencyBuild, /PCRE2_VERSION="10\.47"/)
  assert.match(dependencyBuild, /47fe8c99461250d42f89e6e8fdaeba9da057855d06eb7fc08d9ca03fd08d7bc7/)
  assert.match(dependencyBuild, /LIBICONV_VERSION="1\.19"/)
  assert.match(dependencyBuild, /88dd96a8c0464eca144fc791ae60cd31cd8ee78321e67397e25fc095c4a19aa6/)
  assert.match(dependencyBuild, /IMAGEMAGICK_VERSION="7\.1\.2-30"/)
  assert.match(dependencyBuild, /--with-modules=no/)
  assert.match(dependencyBuild, /macOS SDK zlib/)
  assert.match(dependencyBuild, /"\$INSTALL_PREFIX\/sbin"/)
  assert.match(dylibBundle, /Mach-O dependency is outside the Runtime and allowed prefixes/)
  assert.match(releaseBuild, /build-macos-runtime-packages\.sh/)
  assert.match(releaseBuild, /"\$PACKAGE_MANIFEST"/)
  assert.doesNotMatch(
    releaseBuild,
    /7\.4\.33|8\.2\.33|8\.4\.24|12\.3\.2|20\.20\.2|24\.20\.0/
  )
  assert.match(releaseBuild, /generate-macos/)
  assert.match(releaseBuild, /fabdev-runtime-v1\.json/)
  assert.match(releaseBuild, /pwd -P/)
  assert.doesNotMatch(releaseBuild, /gh release|--draft|--publish/)
  assert.match(phpPackage, /validate-macos-runtime-minimum\.sh/)
  assert.match(phpPackage, /FABDEV_RUNTIME_DEPENDENCY_PREFIX/)
  assert.match(phpPackage, /validate-php-runtime-health\.sh/)
  assert.match(phpHealthCheck, /stream_socket_client\("unix:\/\/"/)
  assert.match(phpHealthCheck, /"fpm-fcgi"/)
  assert.match(phpHealthCheck, /strpos\(\$headers,/)
  assert.doesNotMatch(phpHealthCheck, /str_contains/)
  assert.match(phpHealthCheck, /PHP-FPM FastCGI request passed/)
  assert.match(phpSiteHealthCheck, /fastcgi_pass unix:/)
  assert.match(phpSiteHealthCheck, /Host: \$SITE_DOMAIN/)
  assert.match(phpSiteHealthCheck, /Nginx Site HTTP passed with PHP/)
  assert.match(phpSiteHealthCheck, /PHP Runtime Site HTTP health check passed/)
  assert.match(phpOnlineFlowCheck, /FABDEV_MACOS_PHP_RUNTIME_PACKAGE/)
  assert.match(
    phpOnlineFlowCheck,
    /streams_the_real_macos_php_package_over_loopback/
  )
  assert.match(
    phpOnlineFlowCheck,
    /installs_real_macos_php_through_the_online_agent_protocol/
  )
  assert.match(mariaDbPackage, /validate-macos-runtime-minimum\.sh/)
  assert.match(mariaDbPackage, /validate-mariadb-runtime-health\.sh/)
  assert.match(mariaDbHealthCheck, /mariadb-install-db/)
  assert.match(mariaDbHealthCheck, /SELECT VERSION\(\), @@version_comment, 1 \+ 1/)
  assert.match(mariaDbHealthCheck, /mariadb-admin/)
  assert.match(nodePackage, /validate-macos-runtime-minimum\.sh/)
  assert.match(minimumVersionCheck, /vtool -show-build/)
  assert.match(minimumVersionCheck, /incompatible Mach-O minimum version declaration/)
})

test('removes every exact stale fabDev CA without requiring user data', async () => {
  const uninstaller = await readFile(
    join(repoRoot, 'distribution/macos/community/Uninstall-fabDev.command'),
    'utf8'
  )

  assert.match(uninstaller, /security find-certificate -c "\$common_name" -p "\$keychain"/)
  assert.match(uninstaller, /expected_identity="CN=\$common_name,O=fabDev"/)
  assert.match(uninstaller, /sed 's\/\^subject= \*\/\/'/)
  assert.match(uninstaller, /sed 's\/\^issuer= \*\/\/'/)
  assert.match(uninstaller, /security delete-certificate -t -Z "\$fingerprint"/)
  assert.doesNotMatch(uninstaller, /DATA_ROOT\/config\/tls\/ca\.crt/)
})

test('allows macOS updates when new and legacy app names resolve to the same bundle', async () => {
  const installer = await readFile(
    join(repoRoot, 'distribution/macos/community/Install-fabDev.command'),
    'utf8'
  )

  assert.match(
    installer,
    /\[\[ -d "\$APP_TARGET" && -d "\$LEGACY_APP_TARGET" \]\] &&\n  \[\[ ! "\$APP_TARGET" -ef "\$LEGACY_APP_TARGET" \]\]/
  )
})
