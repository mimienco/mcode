import { resolve } from 'node:path'

/**
 * Benchmark scripts live at tests/tools/benchmarks, three levels below the
 * application root that owns package.json and out/.
 */
export function resolveBenchmarkRepoRoot(benchmarkDirectory) {
  return resolve(benchmarkDirectory, '..', '..', '..')
}
