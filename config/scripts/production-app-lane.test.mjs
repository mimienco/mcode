import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, describe, expect, it } from 'vitest'
import {
  createProductionBuildEnv,
  createProductionLaunchEnv,
  resolveProductionAppArtifact
} from './production-app-lane.mjs'

const tempRoots = []

afterEach(async () => {
  await Promise.all(tempRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })))
})

describe('production app lane', () => {
  it('launches only the isolated packaged Windows production artifact', async () => {
    const repoRoot = await mkdtemp(join(tmpdir(), 'mcode-production-lane-'))
    tempRoots.push(repoRoot)
    const legacyArtifact = join(repoRoot, 'dist', 'win-unpacked', 'MCode.exe')
    await mkdir(join(repoRoot, 'dist', 'win-unpacked'), { recursive: true })
    await writeFile(legacyArtifact, 'legacy')

    expect(() => resolveProductionAppArtifact({ repoRoot, platform: 'win32' })).toThrow(
      /production build is missing/i
    )

    const productionArtifact = join(repoRoot, 'dist', 'prod', 'win-unpacked', 'MCode.exe')
    await mkdir(join(repoRoot, 'dist', 'prod', 'win-unpacked'), { recursive: true })
    await writeFile(productionArtifact, 'production')

    expect(resolveProductionAppArtifact({ repoRoot, platform: 'win32' })).toBe(
      productionArtifact
    )
  })

  it('removes inherited dev profile identity before launching production', () => {
    expect(
      createProductionLaunchEnv({
        PATH: 'bin',
        MCODE_USER_DATA_PATH: 'stale-user-data',
        MCODE_DEV_USER_DATA_PATH: 'dev-user-data',
        MCODE_DEV_INSTANCE_LABEL: 'dev-label',
        mcode_dev_branch: 'lowercase-dev'
      })
    ).toEqual({ PATH: 'bin', MCODE_BUILD_LANE: 'production' })
  })

  it('removes inherited release-channel settings before building production', () => {
    expect(
      createProductionBuildEnv({
        PATH: 'bin',
        MCODE_MAC_RELEASE: '1',
        MCODE_LINUX_ARM64_RELEASE: '1',
        mcode_win_hourly: '1',
        MCODE_HOURLY_BUILD_VERSION: '9.9.9'
      })
    ).toEqual({ PATH: 'bin', MCODE_BUILD_LANE: 'production' })
  })
})
