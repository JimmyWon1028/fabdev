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
const bundledRuntimes = [
  { name: 'dnsmasq', version: '2.93' },
  { name: 'nginx', version: '1.30.4' },
  { name: 'php', version: '7.4.33' },
  { name: 'php', version: '8.2.33' }
]

if (process.platform !== 'darwin') {
  process.stdout.write('Skipping bundled macOS Runtime preparation on this platform\n')
  process.exit(0)
}

mkdirSync(outputRoot, { recursive: true })

for (const entry of readdirSync(outputRoot, { withFileTypes: true })) {
  if (
    entry.isFile() &&
    (/^(dnsmasq|nginx|php|mariadb)-.+\.(json|tar\.gz)$/.test(entry.name) ||
      entry.name === 'catalog.json')
  ) {
    rmSync(join(outputRoot, entry.name))
  }
}

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
