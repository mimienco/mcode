import { spawn } from 'node:child_process'
import { existsSync } from 'node:fs'
import path from 'node:path'
import { pathToFileURL } from 'node:url'

const repoRoot = path.resolve(import.meta.dirname, '../..')
const productionOutputRoot = (...segments) => path.join('dist', 'prod', ...segments)

function productionArtifactCandidates(root, platform) {
  if (platform === 'win32') {
    return [path.join(root, productionOutputRoot('win-unpacked', 'MCode.exe'))]
  }
  if (platform === 'darwin') {
    return ['mac', 'mac-arm64', 'mac-x64', 'mac-universal'].map((directory) =>
      path.join(root, productionOutputRoot(directory, 'MCode.app'))
    )
  }
  if (platform === 'linux') {
    return [path.join(root, productionOutputRoot('linux-unpacked', 'mcode-ide'))]
  }
  throw new Error(`Production app launch is unsupported on ${platform}`)
}

export function resolveProductionAppArtifact({
  repoRoot: root = repoRoot,
  platform = process.platform
} = {}) {
  const candidates = productionArtifactCandidates(path.resolve(root), platform)
  const artifact = candidates.find((candidate) => existsSync(candidate))
  if (artifact) {
    return artifact
  }
  throw new Error(
    `Production build is missing. Run \`pnpm app:prod:build\` first. Expected ${candidates.join(' or ')}`
  )
}

export function createProductionLaunchEnv(source = process.env) {
  const env = { ...source }
  for (const key of Object.keys(env)) {
    const normalizedKey = key.toUpperCase()
    if (normalizedKey.startsWith('MCODE_DEV_') || normalizedKey === 'MCODE_USER_DATA_PATH') {
      delete env[key]
    }
  }
  env.MCODE_BUILD_LANE = 'production'
  return env
}

export function createProductionBuildEnv(source = process.env) {
  const env = createProductionLaunchEnv(source)
  for (const key of [
    'MCODE_MAC_RELEASE',
    'MCODE_LINUX_ARM64_RELEASE',
    'MCODE_MAC_HOURLY',
    'MCODE_MAC_DAILY',
    'MCODE_MAC_ADHOC',
    'MCODE_WIN_HOURLY',
    'MCODE_WIN_DAILY',
    'MCODE_WIN_ADHOC',
    'MCODE_HOURLY_BUILD_VERSION',
    'MCODE_DAILY_BUILD_VERSION',
    'MCODE_ADHOC_BUILD_VERSION',
    'MCODE_LOCAL_BUILD_VERSION'
  ]) {
    for (const inheritedKey of Object.keys(env)) {
      if (inheritedKey.toUpperCase() === key) {
        delete env[inheritedKey]
      }
    }
  }
  return env
}

function waitForChild(child, action) {
  return new Promise((resolvePromise, reject) => {
    child.once('error', reject)
    child.once('exit', (code, signal) => {
      if (code === 0) {
        resolvePromise()
        return
      }
      reject(new Error(`${action} failed${signal ? ` with ${signal}` : ` with exit code ${code}`}`))
    })
  })
}

async function buildProductionApp() {
  const packageManagerEntry = process.env.npm_execpath
  if (!packageManagerEntry) {
    throw new Error('Run the production build through `pnpm app:prod:build`.')
  }
  const child = spawn(process.execPath, [packageManagerEntry, 'run', 'build:unpack'], {
    cwd: repoRoot,
    env: createProductionBuildEnv(),
    stdio: 'inherit'
  })
  await waitForChild(child, 'Production build')
  console.log(`[mcode-prod] Built ${path.join(repoRoot, productionOutputRoot())}`)
}

async function launchProductionApp() {
  const artifact = resolveProductionAppArtifact()
  const child =
    process.platform === 'darwin'
      ? spawn('open', [artifact], { env: createProductionLaunchEnv(), stdio: 'ignore' })
      : spawn(artifact, [], {
          detached: true,
          env: createProductionLaunchEnv(),
          stdio: 'ignore',
          windowsHide: false
        })
  await new Promise((resolvePromise, reject) => {
    child.once('error', reject)
    child.once('spawn', resolvePromise)
  })
  child.unref()
  console.log(`[mcode-prod] Launched ${artifact}`)
}

async function main(command) {
  if (command === 'build') {
    await buildProductionApp()
    return
  }
  if (command === 'run') {
    await launchProductionApp()
    return
  }
  throw new Error('Usage: production-app-lane.mjs <build|run>')
}

const isDirectExecution =
  process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url
if (isDirectExecution) {
  main(process.argv[2]).catch((error) => {
    console.error(`[mcode-prod] ${error instanceof Error ? error.message : String(error)}`)
    process.exitCode = 1
  })
}
