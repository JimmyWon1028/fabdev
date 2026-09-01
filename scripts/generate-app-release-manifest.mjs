import { createHash } from 'node:crypto'
import { createReadStream } from 'node:fs'
import {
  copyFile,
  lstat,
  mkdir,
  mkdtemp,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  writeFile
} from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptPath = fileURLToPath(import.meta.url)
const scriptDir = dirname(scriptPath)
const defaultRepoRoot = resolve(scriptDir, '..')
const defaultRepository = 'JimmyWon1028/fabdev'

function parseArgs(argv) {
  const options = {}
  const names = new Map([
    ['--version', 'version'],
    ['--published-at', 'publishedAt'],
    ['--output-dir', 'outputDir'],
    ['--repository', 'repository'],
    ['--macos-arm64', 'macosArm64'],
    ['--windows-x64', 'windowsX64'],
    ['--windows-connect-x64', 'windowsConnectX64'],
    ['--runtime-package-dir', 'runtimePackageDirs']
  ])

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index]
    if (argument === '--help') {
      options.help = true
      continue
    }
    const name = names.get(argument)
    if (!name) {
      throw new Error(`Unknown option: ${argument}`)
    }
    if (name !== 'runtimePackageDirs' && options[name] !== undefined) {
      throw new Error(`Duplicate option: ${argument}`)
    }
    const value = argv[index + 1]
    if (!value || value.startsWith('--')) {
      throw new Error(`Missing value for ${argument}`)
    }
    if (name === 'runtimePackageDirs') {
      options.runtimePackageDirs ??= []
      options.runtimePackageDirs.push(value)
    } else {
      options[name] = value
    }
    index += 1
  }

  return options
}

function printHelp() {
  process.stdout.write(`Usage:
  node scripts/generate-app-release-manifest.mjs \\
    --version <semver> \\
    --published-at <UTC RFC3339> \\
    --output-dir <new-directory> \\
    [--repository <owner/repository>] \\
    [--macos-arm64 <dmg>] \\
    [--windows-x64 <setup.exe>] \\
    [--windows-connect-x64 <fabdev-connect.exe>] \\
    [--runtime-package-dir <directory>]...

At least one App installer must be provided. The output directory must not exist.
`)
}

function requireString(value, name) {
  if (typeof value !== 'string' || value.trim() === '') {
    throw new Error(`Missing required option: ${name}`)
  }
  return value.trim()
}

function validateVersion(version) {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error(`Stable release version must be SemVer without a prerelease suffix: ${version}`)
  }
}

function validatePublishedAt(publishedAt) {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(publishedAt)) {
    throw new Error(`Published time must use UTC RFC 3339 seconds: ${publishedAt}`)
  }
  const parsed = new Date(publishedAt)
  if (Number.isNaN(parsed.getTime()) || parsed.toISOString().replace('.000Z', 'Z') !== publishedAt) {
    throw new Error(`Published time is invalid: ${publishedAt}`)
  }
}

function validateRepository(repository) {
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository)) {
    throw new Error(`Repository must use owner/name format: ${repository}`)
  }
}

async function readJson(path) {
  return JSON.parse(await readFile(path, 'utf8'))
}

async function readProjectMetadata(repoRoot) {
  const rootPackage = await readJson(join(repoRoot, 'package.json'))
  const desktopPackage = await readJson(join(repoRoot, 'apps/desktop/package.json'))
  const tauriConfig = await readJson(join(repoRoot, 'apps/desktop/src-tauri/tauri.conf.json'))
  const cargoManifest = await readFile(join(repoRoot, 'Cargo.toml'), 'utf8')
  const cargoSection = cargoManifest.match(/\[workspace\.package\]([\s\S]*?)(?:\n\[|$)/)?.[1]
  const cargoVersion = cargoSection?.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  const rustProtocol = await readFile(join(repoRoot, 'crates/core/src/protocol.rs'), 'utf8')
  const typescriptProtocol = await readFile(join(repoRoot, 'packages/contracts/src/index.ts'), 'utf8')
  const rustProtocolVersion = rustProtocol.match(/pub const PROTOCOL_VERSION: u16 = (\d+);/)?.[1]
  const typescriptProtocolVersion = typescriptProtocol.match(/export const protocolVersion = (\d+)/)?.[1]

  const versions = [rootPackage.version, desktopPackage.version, tauriConfig.version, cargoVersion]
  if (versions.some((version) => typeof version !== 'string')) {
    throw new Error('Unable to read all four project version sources')
  }
  if (new Set(versions).size !== 1) {
    throw new Error(`Project version sources do not match: ${versions.join(', ')}`)
  }
  if (!rustProtocolVersion || rustProtocolVersion !== typescriptProtocolVersion) {
    throw new Error(
      `Rust and TypeScript Agent Protocol versions do not match: ${rustProtocolVersion ?? 'missing'}, ${typescriptProtocolVersion ?? 'missing'}`
    )
  }

  return {
    version: versions[0],
    agentProtocolVersion: Number.parseInt(rustProtocolVersion, 10)
  }
}

async function pathExists(path) {
  try {
    await lstat(path)
    return true
  } catch (error) {
    if (error?.code === 'ENOENT') {
      return false
    }
    throw error
  }
}

async function validateSource(path, label) {
  const source = resolve(path)
  const sourceStat = await lstat(source)
  if (!sourceStat.isFile() || sourceStat.isSymbolicLink()) {
    throw new Error(`${label} must be a regular file: ${source}`)
  }
  if (sourceStat.size === 0) {
    throw new Error(`${label} must not be empty: ${source}`)
  }
  return source
}

async function sha256File(path) {
  const hash = createHash('sha256')
  for await (const chunk of createReadStream(path)) {
    hash.update(chunk)
  }
  return hash.digest('hex')
}

function installerDefinitions(version, options) {
  return [
    {
      source: options.macosArm64,
      label: 'macOS ARM64 installer',
      fileName: `fabDev-Community-${version}-macos-arm64.dmg`,
      manifest: {
        platform: 'macos',
        architecture: 'arm64',
        minimumOsVersion: '13.0',
        installMode: 'open-dmg'
      }
    },
    {
      source: options.windowsX64,
      label: 'Windows x64 installer',
      fileName: `fabDev-Community-${version}-windows-x64-setup.exe`,
      manifest: {
        platform: 'windows',
        architecture: 'x64',
        minimumOsVersion: '11',
        installMode: 'run-installer-after-quit'
      }
    }
  ].filter((definition) => definition.source)
}

function optionalToolDefinitions(version, options) {
  return [
    {
      source: options.windowsConnectX64,
      label: 'fabDev Connect Windows x64',
      fileName: `fabDev-Connect-${version}-windows-x64.exe`
    }
  ].filter((definition) => definition.source)
}

async function runtimePackageDefinitions(repoRoot, options) {
  const packageNamePattern =
    /^(php|mariadb|node)-\d+\.\d+\.\d+-(macos-arm64|windows-x64)-community\.tar\.gz$/
  const definitions = []
  const fileNames = new Set()

  for (const directoryOption of options.runtimePackageDirs ?? []) {
    const directory = resolve(repoRoot, directoryOption)
    const directoryStat = await lstat(directory)
    if (!directoryStat.isDirectory() || directoryStat.isSymbolicLink()) {
      throw new Error(`Runtime package directory must be a real directory: ${directory}`)
    }

    const entries = await readdir(directory, { withFileTypes: true })
    for (const entry of entries) {
      if (!entry.isFile() || !packageNamePattern.test(entry.name)) {
        continue
      }
      if (fileNames.has(entry.name)) {
        throw new Error(`Duplicate Runtime package filename: ${entry.name}`)
      }
      fileNames.add(entry.name)
      definitions.push({
        source: join(directory, entry.name),
        label: `Runtime package ${entry.name}`,
        fileName: entry.name
      })
    }
  }

  return definitions.sort((left, right) => left.fileName.localeCompare(right.fileName))
}

export async function prepareAppRelease(options) {
  const repoRoot = resolve(options.repoRoot ?? defaultRepoRoot)
  const version = requireString(options.version, '--version')
  const publishedAt = requireString(options.publishedAt, '--published-at')
  const outputDir = resolve(repoRoot, requireString(options.outputDir, '--output-dir'))
  const repository = (options.repository ?? defaultRepository).trim()

  validateVersion(version)
  validatePublishedAt(publishedAt)
  validateRepository(repository)

  const projectMetadata = await readProjectMetadata(repoRoot)
  if (version !== projectMetadata.version) {
    throw new Error(
      `Release version ${version} does not match project version ${projectMetadata.version}`
    )
  }

  const installers = installerDefinitions(version, options)
  if (installers.length === 0) {
    throw new Error('At least one App installer is required')
  }
  const optionalTools = optionalToolDefinitions(version, options)
  const runtimePackages = await runtimePackageDefinitions(repoRoot, options)
  const definitions = [...installers, ...optionalTools, ...runtimePackages]
  for (const definition of definitions) {
    definition.source = await validateSource(resolve(repoRoot, definition.source), definition.label)
  }

  if (await pathExists(outputDir)) {
    throw new Error(`Output directory already exists: ${outputDir}`)
  }

  const outputParent = dirname(outputDir)
  await mkdir(outputParent, { recursive: true })
  const stagingDir = await mkdtemp(join(outputParent, '.fabdev-release-'))

  try {
    const releaseFiles = []
    const manifestArtifacts = []

    for (const definition of definitions) {
      const destination = join(stagingDir, definition.fileName)
      await copyFile(definition.source, destination)
      const destinationStat = await stat(destination)
      const sha256 = await sha256File(destination)
      await writeFile(
        `${destination}.sha256`,
        `${sha256}  ${definition.fileName}\n`,
        'utf8'
      )
      releaseFiles.push({ fileName: definition.fileName, sha256 })

      if (definition.manifest) {
        manifestArtifacts.push({
          platform: definition.manifest.platform,
          architecture: definition.manifest.architecture,
          minimumOsVersion: definition.manifest.minimumOsVersion,
          fileName: definition.fileName,
          url: `https://github.com/${repository}/releases/download/v${version}/${definition.fileName}`,
          size: destinationStat.size,
          sha256,
          signature: null,
          installMode: definition.manifest.installMode
        })
      }
    }

    const manifest = {
      schemaVersion: 1,
      product: 'fabdev',
      channel: 'stable',
      version,
      tag: `v${version}`,
      publishedAt,
      releaseUrl: `https://github.com/${repository}/releases/tag/v${version}`,
      releaseNotesUrl: `https://github.com/${repository}/releases/tag/v${version}`,
      unsignedCommunityBuild: true,
      integrity: 'sha256',
      compatibility: {
        agentProtocolVersion: projectMetadata.agentProtocolVersion,
        requiresFullInstaller: true
      },
      artifacts: manifestArtifacts
    }
    const manifestContents = `${JSON.stringify(manifest, null, 2)}\n`
    const checksumContents = releaseFiles
      .map((file) => `${file.sha256}  ${file.fileName}`)
      .join('\n') + '\n'

    await writeFile(join(stagingDir, 'SHA256SUMS'), checksumContents, 'utf8')
    await writeFile(join(stagingDir, 'fabdev-app-v1.json'), manifestContents, 'utf8')
    await writeFile(join(stagingDir, 'fabdev-stable-v1.json'), manifestContents, 'utf8')
    await rename(stagingDir, outputDir)

    return {
      outputDir,
      manifest,
      files: await readdir(outputDir)
    }
  } catch (error) {
    await rm(stagingDir, { force: true, recursive: true })
    throw error
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2))
  if (options.help) {
    printHelp()
    return
  }
  const result = await prepareAppRelease(options)
  process.stdout.write(`Prepared App Release assets: ${result.outputDir}\n`)
  for (const file of result.files.sort()) {
    process.stdout.write(`  ${file}\n`)
  }
}

if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  main().catch((error) => {
    process.stderr.write(`${error.message}\n`)
    process.exitCode = 1
  })
}
