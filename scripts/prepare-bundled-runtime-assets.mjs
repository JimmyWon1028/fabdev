import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync
} from 'node:fs'
import { createHash } from 'node:crypto'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')
const sourceRoot = resolve(
  process.env.FABDEV_BUNDLED_RUNTIME_SOURCE ?? join(repoRoot, 'artifacts')
)
const outputRoot = resolve(
  process.env.FABDEV_BUNDLED_RUNTIME_OUTPUT ??
    join(repoRoot, 'apps/desktop/src-tauri/runtime/macos')
)
const releaseSuffix = sourceRoot.endsWith('/community-runtimes') ? 'community' : 'dev'
const manifestPath = resolve(
  process.env.FABDEV_BUNDLED_RUNTIME_MANIFEST ??
    join(repoRoot, 'resources/runtime-packages/macos-arm64-bundled.json')
)
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'))
const bundledRuntimes = manifest.packages

if (
  manifest.schemaVersion !== 1 ||
  manifest.platform !== 'macos' ||
  manifest.architecture !== 'arm64' ||
  !Array.isArray(bundledRuntimes) ||
  bundledRuntimes.length === 0
) {
  throw new Error('Bundled Runtime manifest must target macOS ARM64')
}
if (
  bundledRuntimes.filter((runtime) => runtime.name === 'php' && runtime.default).length !== 1
) {
  throw new Error('Bundled Runtime manifest must declare exactly one default PHP Runtime')
}

const allowCrossPlatformPreparation =
  process.env.FABDEV_ALLOW_CROSS_PLATFORM_RUNTIME_PREPARATION === '1'

if (process.platform !== 'darwin' && !allowCrossPlatformPreparation) {
  process.stdout.write('Skipping bundled macOS Runtime preparation on this platform\n')
  process.exit(0)
}

mkdirSync(outputRoot, { recursive: true })

for (const entry of readdirSync(outputRoot, { withFileTypes: true })) {
  if (
    entry.isFile() &&
    (/\.(json|tar\.gz)$/.test(entry.name) ||
      entry.name === 'catalog.json' ||
      entry.name === 'manifest.json')
  ) {
    rmSync(join(outputRoot, entry.name))
  }
}

writeFileSync(
  join(outputRoot, 'manifest.json'),
  `${JSON.stringify(
    {
      schemaVersion: manifest.schemaVersion,
      platform: manifest.platform,
      architecture: manifest.architecture,
      packages: bundledRuntimes.map(({ name, version, default: isDefault = false }) => ({
        name,
        version,
        default: isDefault
      }))
    },
    null,
    2
  )}\n`
)

for (const runtime of bundledRuntimes) {
  const sourceStem = `${runtime.name}-${runtime.version}-macos-arm64-${releaseSuffix}`
  const sourceDescriptor = join(sourceRoot, `${sourceStem}.json`)
  const sourceArchive = join(sourceRoot, `${sourceStem}.tar.gz`)
  if (!existsSync(sourceDescriptor) || !existsSync(sourceArchive)) {
    throw new Error(`Bundled Runtime source is incomplete: ${sourceStem}`)
  }

  const release = JSON.parse(readFileSync(sourceDescriptor, 'utf8'))
  validateRelease(release, runtime, sourceArchive)

  const outputStem = `${runtime.name}-${runtime.version}`
  const outputArchiveName = `${outputStem}.tar.gz`
  const outputDescriptor = join(outputRoot, `${outputStem}.json`)
  const outputArchive = join(outputRoot, outputArchiveName)
  copyFileSync(sourceArchive, outputArchive)
  writeFileSync(
    outputDescriptor,
    `${JSON.stringify({ ...release, url: outputArchiveName }, null, 2)}\n`
  )
  process.stdout.write(`Prepared built-in Runtime: ${runtime.name} ${runtime.version}\n`)
}

function validateRelease(release, expected, archivePath) {
  if (
    release.name !== expected.name ||
    release.version !== expected.version ||
    release.platform !== 'macos' ||
    release.architecture !== 'arm64'
  ) {
    throw new Error(`Bundled Runtime descriptor does not match ${expected.name} ${expected.version}`)
  }
  const size = statSync(archivePath).size
  if (release.size !== size) {
    throw new Error(`Bundled Runtime size does not match: ${archivePath}`)
  }
  const checksum = createHash('sha256').update(readFileSync(archivePath)).digest('hex')
  if (release.sha256.toLowerCase() !== checksum) {
    throw new Error(`Bundled Runtime SHA-256 does not match: ${archivePath}`)
  }
}
