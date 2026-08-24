// Attributes the entry's bare `import "./lib-*.js"` edge: intersects the
// entry chunk's source modules with the lib chunk's source modules via
// static import resolution over real files.
const fs = require('fs')
const path = require('path')

const assets = 'out/renderer/assets'
const norm = (s) => s.split('\\').join('/').replace(/^D:\/ADE\/orca\//, '')
const stripMap = (f) => f.replace(/\.map$/, '')

const html = fs.readFileSync('out/renderer/index.html', 'utf8')
const entryJs = /src="\.\/assets\/([^"]+)"/.exec(html)[1]
const pre = [...html.matchAll(/modulepreload[^]*?href="\.\/assets\/([^"]+)"/g)].map((m) => m[1])

const readMap = (chunk) => {
  try {
    return JSON.parse(fs.readFileSync(path.join(assets, chunk + '.map'), 'utf8'))
  } catch {
    return null
  }
}

const entryMap = readMap(entryJs)
const entrySources = entryMap.sources.map(norm)
const libChunk = pre.find((f) => /^lib-/.test(f) && (readMap(f)?.sources || []).some((s) => /unified|remark|micromark|mdast/i.test(norm(s))))
console.log('markdown lib chunk:', libChunk)
const libMap = readMap(libChunk)
const libSources = new Set(libMap.sources.map(norm))
console.log('lib sources:', libSources.size)

// resolve relative/@ imports of a source file
const root = 'src/renderer/src'
function resolveSpec(fromFile, spec) {
  let base
  if (spec.startsWith('@/')) base = path.join(root, spec.slice(2))
  else if (spec.startsWith('.')) base = path.join(path.dirname(fromFile), spec)
  else return null
  base = norm(base)
  for (const c of [base + '.ts', base + '.tsx', base + '.js', base, path.join(base, 'index.ts'), path.join(base, 'index.tsx')]) {
    if (libSources.has(c)) return c
  }
  return null
}

const impRe = /(?:^|[\s;}])import\s+(?:type\s+)?(?:[\s\S]*?from\s+)?['"](\.[^'"]+|@\/[^'"]+)['"]|export\s+[\s\S]*?from\s+['"](\.[^'"]+|@\/[^'"]+)['"]/g

let hits = 0
for (const src of entrySources) {
  if (!fs.existsSync(src)) continue
  const s = fs.readFileSync(src, 'utf8')
  impRe.lastIndex = 0
  let m
  while ((m = impRe.exec(s))) {
    const spec = m[1] || m[2]
    if (!spec) continue
    const r = resolveSpec(src, spec)
    if (r) {
      hits++
      console.log('EDGE:', src, '\n   -> imports', r)
    }
  }
}
console.log('total edges entry->lib:', hits)
