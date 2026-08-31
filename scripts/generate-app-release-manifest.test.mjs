import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { mkdtemp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { fileURLToPath } from 'node:url'

import { prepareAppRelease } from './generate-app-release-manifest.mjs'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const projectVersion = JSON.parse(await readFile(join(repoRoot, 'package.json'), 'utf8')).version

function digest(contents) {
  return createHash('sha256').update(contents).digest('hex')
}

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
  const macRuntimeSource = join(testRoot, 'input-runtime-macos.tar.gz')
  const windowsRuntimeSource = join(testRoot, 'input-runtime-windows.tar.gz')
  const outputDir = join(testRoot, 'release')
  await writeFile(macSource, macContents)
  await writeFile(windowsSource, windowsContents)
  await writeFile(connectSource, connectContents)
  await writeFile(macRuntimeSource, macRuntimeContents)
  await writeFile(windowsRuntimeSource, windowsRuntimeContents)

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-28T12:34:56Z',
    outputDir,
    macosArm64: macSource,
    windowsX64: windowsSource,
    windowsConnectX64: connectSource,
    runtimeMacosArm64: macRuntimeSource,
    runtimeWindowsX64: windowsRuntimeSource
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
  const php74RuntimeSource = join(testRoot, 'input-runtime-php74-windows.tar.gz')
  const php82RuntimeSource = join(testRoot, 'input-runtime-php82-windows.tar.gz')
  const php84RuntimeSource = join(testRoot, 'input-runtime-php84-windows.tar.gz')
  const mariaDbRuntimeSource = join(testRoot, 'input-runtime-mariadb-windows.tar.gz')
  const node20RuntimeSource = join(testRoot, 'input-runtime-node20-windows.tar.gz')
  const node24RuntimeSource = join(testRoot, 'input-runtime-node24-windows.tar.gz')
  const outputDir = join(testRoot, 'release')
  await writeFile(windowsSource, 'Windows installer fixture')
  await writeFile(connectSource, 'fabDev Connect fixture')
  await writeFile(php74RuntimeSource, 'Windows PHP 7.4 Runtime fixture')
  await writeFile(php82RuntimeSource, 'Windows PHP 8.2 Runtime fixture')
  await writeFile(php84RuntimeSource, 'Windows PHP 8.4 Runtime fixture')
  await writeFile(mariaDbRuntimeSource, 'Windows MariaDB Runtime fixture')
  await writeFile(node20RuntimeSource, 'Windows Node.js 20 Runtime fixture')
  await writeFile(node24RuntimeSource, 'Windows Node.js 24 Runtime fixture')

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-30T12:34:56Z',
    outputDir,
    windowsX64: windowsSource,
    windowsConnectX64: connectSource,
    runtimePhp74WindowsX64: php74RuntimeSource,
    runtimePhp82WindowsX64: php82RuntimeSource,
    runtimeWindowsX64: php84RuntimeSource,
    runtimeMariaDbWindowsX64: mariaDbRuntimeSource,
    runtimeNode20WindowsX64: node20RuntimeSource,
    runtimeNode24WindowsX64: node24RuntimeSource
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

test('prepares a macOS ARM64 release with all online Runtime packages', async (context) => {
  const testRoot = await mkdtemp(join(tmpdir(), 'fabdev-release-macos-test-'))
  context.after(async () => rm(testRoot, { force: true, recursive: true }))
  const macSource = join(testRoot, 'input.dmg')
  const phpRuntimeSource = join(testRoot, 'input-runtime-php-macos.tar.gz')
  const mariaDbRuntimeSource = join(testRoot, 'input-runtime-mariadb-macos.tar.gz')
  const node20RuntimeSource = join(testRoot, 'input-runtime-node20-macos.tar.gz')
  const node24RuntimeSource = join(testRoot, 'input-runtime-node24-macos.tar.gz')
  const outputDir = join(testRoot, 'release')
  await writeFile(macSource, 'macOS installer fixture')
  await writeFile(phpRuntimeSource, 'macOS PHP Runtime fixture')
  await writeFile(mariaDbRuntimeSource, 'macOS MariaDB Runtime fixture')
  await writeFile(node20RuntimeSource, 'macOS Node.js 20 Runtime fixture')
  await writeFile(node24RuntimeSource, 'macOS Node.js 24 Runtime fixture')

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-31T06:00:00Z',
    outputDir,
    macosArm64: macSource,
    runtimeMacosArm64: phpRuntimeSource,
    runtimeMariaDbMacosArm64: mariaDbRuntimeSource,
    runtimeNode20MacosArm64: node20RuntimeSource,
    runtimeNode24MacosArm64: node24RuntimeSource
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
  assert.match(workflow, /build-windows-php-runtime\.ps1 -OutputDirectory release-input/)
  assert.match(workflow, /FABDEV_WINDOWS_PHP74_RUNTIME_PACKAGE:/)
  assert.match(workflow, /FABDEV_WINDOWS_PHP82_RUNTIME_PACKAGE:/)
  assert.match(workflow, /FABDEV_WINDOWS_PHP84_RUNTIME_PACKAGE:/)
  assert.match(workflow, /installs_real_windows_php_archive/)
  assert.match(workflow, /--runtime-php74-windows-x64/)
  assert.match(workflow, /--runtime-php82-windows-x64/)
  assert.match(workflow, /--runtime-windows-x64/)
  assert.match(workflow, /FABDEV_WINDOWS_RUNTIME_NAMES: mariadb node/)
  assert.match(workflow, /--runtime-mariadb-windows-x64/)
  assert.match(workflow, /--runtime-node20-windows-x64/)
  assert.match(workflow, /--runtime-node24-windows-x64/)
  assert.match(workflow, /build-macos:/)
  assert.match(workflow, /draft-macos-arm64/)
  assert.match(workflow, /draft-macos-online-runtimes/)
  assert.match(workflow, /build-macos-online-runtime-packages\.sh/)
  assert.match(workflow, /--macos-arm64/)
  assert.match(workflow, /--runtime-macos-arm64/)
  assert.match(workflow, /--runtime-mariadb-macos-arm64/)
  assert.match(workflow, /--runtime-node20-macos-arm64/)
  assert.match(workflow, /--runtime-node24-macos-arm64/)
  assert.match(workflow, /generate-community/)
  assert.match(workflow, /--bin fabdev-runtime-catalog/)
  assert.match(workflow, /release-assets\/fabdev-runtime-v1\.json/)
  assert.match(workflow, /\)" = "30"/)

  const usesLines = workflow
    .split('\n')
    .filter((line) => /^\s+uses:/.test(line))
  assert.ok(usesLines.length > 0)
  for (const line of usesLines) {
    assert.match(line, /^\s+uses: [^@\s]+@[0-9a-f]{40}(?:\s+#.*)?$/)
  }
})

test('launches the Windows updater only after fabDev exits', async () => {
  const desktopSource = await readFile(
    join(repoRoot, 'apps/desktop/src-tauri/src/lib.rs'),
    'utf8'
  )

  assert.match(
    desktopSource,
    /const WINDOWS_UPDATE_INSTALLER_ARGUMENTS: \[&str; 3\] = \["\/UPDATE", "\/P", "\/R"\]/
  )
  assert.match(desktopSource, /Wait-Process -Id \$args\[0\]/)
  assert.match(desktopSource, /Start-Process -FilePath \$args\[1\]/)
  assert.match(desktopSource, /\.arg\(std::process::id\(\)\.to_string\(\)\)/)
})

test('pins and verifies all Windows online PHP Runtime packages', async () => {
  const script = await readFile(
    join(repoRoot, 'scripts/build-windows-php-runtime.ps1'),
    'utf8'
  )

  assert.match(script, /Version = "7\.4\.33"/)
  assert.match(
    script,
    /Sha256 = "14ae3250d4447c8ccfc4c45a70d90adfbcd61e728d85f0be56a7ddf8f9c8aace"/
  )
  assert.match(script, /Version = "8\.2\.33"/)
  assert.match(
    script,
    /Sha256 = "d0bd189522fa50255ee94ed4b340ed4330f5ae33a90a74205275b0f0b221d388"/
  )
  assert.match(script, /Version = "8\.4\.24"/)
  assert.match(
    script,
    /Sha256 = "86470a30cbbaeafb259e727dfa5cd336f2f3f0a462cd6f8e3eac00fdbded13cb"/
  )
  assert.match(script, /php-\$phpVersion-windows-x64-community\.tar\.gz/)
  assert.match(script, /ext\/php_mysqli\.dll/)
  assert.match(script, /ext\/php_pdo_mysql\.dll/)
  assert.match(script, /extension_loaded\('mysqli'\)/)
  assert.match(script, /extension_loaded\('pdo_mysql'\)/)
  assert.match(script, /tar\.exe -tzf/)
  assert.match(script, /foreach \(\$runtime in \$phpRuntimes\)/)
  assert.doesNotMatch(script, /mariadb|node/i)
})

test('pins and prepares every macOS ARM64 online Runtime package', async () => {
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
  assert.match(releaseBuild, /FABDEV_RUNTIME_PACKAGE_VARIANT=community/)
  assert.match(releaseBuild, /PHP_VERSION=8\.4\.24/)
  assert.match(releaseBuild, /MARIADB_VERSION=12\.3\.2/)
  assert.match(releaseBuild, /for node_version in 20\.20\.2 24\.20\.0/)
  assert.match(releaseBuild, /generate-macos/)
  assert.match(releaseBuild, /fabdev-runtime-v1\.json/)
  assert.match(releaseBuild, /pwd -P/)
  assert.doesNotMatch(releaseBuild, /gh release|--draft|--publish/)
  assert.match(phpPackage, /validate-macos-runtime-minimum\.sh/)
  assert.match(phpPackage, /FABDEV_RUNTIME_DEPENDENCY_PREFIX/)
  assert.match(phpPackage, /validate-php-runtime-health\.sh/)
  assert.match(phpHealthCheck, /stream_socket_client\("unix:\/\/"/)
  assert.match(phpHealthCheck, /"fpm-fcgi"/)
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
