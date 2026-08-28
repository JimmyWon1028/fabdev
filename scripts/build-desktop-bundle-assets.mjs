import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const scriptDir = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(scriptDir, '..')

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env: process.env,
    stdio: 'inherit',
    shell: false,
    ...options
  })
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    process.exit(result.status ?? 1)
  }
}

function rustTool(name) {
  const result = spawnSync('rustup', ['which', name], {
    cwd: repoRoot,
    encoding: 'utf8',
    shell: false
  })
  if (result.status === 0 && result.stdout.trim()) {
    return result.stdout.trim()
  }
  return name
}

function rustHostTriple(rustc) {
  const result = spawnSync(rustc, ['-vV'], {
    cwd: repoRoot,
    encoding: 'utf8',
    shell: false
  })
  if (result.error) {
    throw result.error
  }
  if (result.status !== 0) {
    process.stderr.write(result.stderr)
    process.exit(result.status ?? 1)
  }
  const host = result.stdout.match(/^host:\s+(.+)$/m)?.[1]
  if (!host) {
    throw new Error('Unable to determine Rust host triple')
  }
  return host
}

run('pnpm', ['--dir', join(repoRoot, 'apps/desktop'), 'build'], {
  shell: process.platform === 'win32'
})

if (process.platform === 'darwin') {
  run(process.execPath, [join(scriptDir, 'prepare-bundled-runtime-assets.mjs')])
  run(join(scriptDir, 'build-macos-helper.sh'), [])
}

const cargo = rustTool('cargo')
const rustc = rustTool('rustc')
const rustdoc = rustTool('rustdoc')
const rustEnvironment = {
  ...process.env,
  CARGO_PROFILE_RELEASE_STRIP: 'none',
  RUSTC: rustc,
  RUSTDOC: rustdoc
}
const hostTriple = rustHostTriple(rustc)
const targetTriple = process.env.TAURI_ENV_TARGET_TRIPLE || hostTriple
const cargoArgs = ['build', '-p', 'fabdev-agent', '--release']
if (targetTriple !== hostTriple) {
  cargoArgs.push('--target', targetTriple)
}
run(cargo, cargoArgs, { env: rustEnvironment })

if (targetTriple.includes('windows')) {
  const helperArgs = ['build', '-p', 'fabdev-windows-helper', '--release']
  if (targetTriple !== hostTriple) {
    helperArgs.push('--target', targetTriple)
  }
  run(cargo, helperArgs, { env: rustEnvironment })
}

const extension = targetTriple.includes('windows') ? '.exe' : ''
const buildOutput = targetTriple === hostTriple
  ? join(repoRoot, 'target/release')
  : join(repoRoot, 'target', targetTriple, 'release')
const sourceAgent = join(buildOutput, `fabdev-agent${extension}`)
const binaryDir = join(repoRoot, 'apps/desktop/src-tauri/binaries')
const destinationAgent = join(binaryDir, `fabdev-agent-${targetTriple}${extension}`)
if (!existsSync(sourceAgent)) {
  throw new Error(`fabDev Agent build output is missing: ${sourceAgent}`)
}
mkdirSync(binaryDir, { recursive: true })
copyFileSync(sourceAgent, destinationAgent)
process.stdout.write(`Prepared Desktop Agent sidecar: ${destinationAgent}\n`)

if (targetTriple.includes('windows')) {
  const sourceHelper = join(buildOutput, 'fabdev-windows-helper.exe')
  const destinationHelper = join(binaryDir, `fabdev-windows-helper-${targetTriple}.exe`)
  if (!existsSync(sourceHelper)) {
    throw new Error(`fabDev Windows Helper build output is missing: ${sourceHelper}`)
  }
  copyFileSync(sourceHelper, destinationHelper)
  process.stdout.write(`Prepared Windows Helper sidecar: ${destinationHelper}\n`)
}
