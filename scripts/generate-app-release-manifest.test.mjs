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
  const macSource = join(testRoot, 'input.dmg')
  const windowsSource = join(testRoot, 'input-setup.exe')
  const connectSource = join(testRoot, 'input-connect.exe')
  const outputDir = join(testRoot, 'release')
  await writeFile(macSource, macContents)
  await writeFile(windowsSource, windowsContents)
  await writeFile(connectSource, connectContents)

  const result = await prepareAppRelease({
    repoRoot,
    version: projectVersion,
    publishedAt: '2026-08-28T12:34:56Z',
    outputDir,
    macosArm64: macSource,
    windowsX64: windowsSource,
    windowsConnectX64: connectSource
  })

  const macName = `fabDev-Community-${projectVersion}-macos-arm64.dmg`
  const windowsName = `fabDev-Community-${projectVersion}-windows-x64-setup.exe`
  const connectName = `fabDev-Connect-${projectVersion}-windows-x64.exe`
  assert.deepEqual((await readdir(outputDir)).sort(), [
    'SHA256SUMS',
    'fabdev-app-v1.json',
    'fabdev-stable-v1.json',
    connectName,
    `${connectName}.sha256`,
    macName,
    `${macName}.sha256`,
    windowsName,
    `${windowsName}.sha256`
  ].sort())

  assert.equal(await readFile(join(outputDir, macName), 'utf8'), macContents.toString())
  assert.equal(await readFile(join(outputDir, windowsName), 'utf8'), windowsContents.toString())
  assert.equal(await readFile(join(outputDir, connectName), 'utf8'), connectContents.toString())

  const manifest = JSON.parse(await readFile(join(outputDir, 'fabdev-app-v1.json'), 'utf8'))
  const stableManifest = JSON.parse(
    await readFile(join(outputDir, 'fabdev-stable-v1.json'), 'utf8')
  )
  assert.deepEqual(stableManifest, manifest)
  assert.equal(manifest.version, projectVersion)
  assert.equal(manifest.tag, `v${projectVersion}`)
  assert.equal(manifest.publishedAt, '2026-08-28T12:34:56Z')
  assert.equal(manifest.compatibility.agentProtocolVersion, 32)
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
      `${digest(connectContents)}  ${connectName}\n`
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
