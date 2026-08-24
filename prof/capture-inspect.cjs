// Captures a main-process CPU profile via --inspect (reliable on Windows where
// --cpu-prof doesn't flush on kill).
const { spawn } = require('child_process')
const path = require('path')
const os = require('os')
const http = require('http')
const WebSocket = require(require.resolve('ws', { paths: [path.resolve(__dirname, '..')] }))

const fixture = path.join(os.tmpdir(), 'mcode-startup-bench', 'userdata-28000-none-0-gh0')
const isolatedHome = path.join(fixture, 'home')

const child = spawn(
  require('electron'),
  ['--inspect=0', path.resolve(__dirname, '..')],
  {
    cwd: path.resolve(__dirname, '..'),
    env: {
      ...process.env,
      MCODE_STARTUP_DIAGNOSTICS: '1',
      MCODE_E2E_USER_DATA_DIR: fixture,
      MCODE_E2E_HEADLESS: '1',
      HOME: isolatedHome,
      USERPROFILE: isolatedHome
    },
    stdio: ['ignore', 'ignore', 'pipe']
  }
)

let buf = ''
let ws = null
let sawLoad = false
child.stderr.on('data', (d) => {
  buf += d
  process.stdout.write(d)
  const m = /ws:\/\/127\.0\.0\.1:(\d+)\/([\w-]+)/.exec(buf)
  if (m && !ws) connect(m[1], m[2])
  if (!sawLoad && /\[startup\] did-finish-load/.test(buf)) {
    sawLoad = true
    setTimeout(finish, 5000)
  }
})

function connect(port, id) {
  http.get({ host: '127.0.0.1', port, path: '/json/list' }, (res) => {
    let body = ''
    res.on('data', (c) => (body += c))
    res.on('end', () => {
      const targets = JSON.parse(body)
      const t = targets.find((x) => x.type === 'NodeMain') || targets[0]
      ws = new WebSocket(t.webSocketDebuggerUrl)
      ws.on('open', () => { send('Profiler.enable'); send('Profiler.start') })
      ws.on('message', (raw) => {
        const msg = JSON.parse(raw); 
        if (msg.result && msg.result.profile) {
          require('fs').writeFileSync(
            path.join(__dirname, 'startup-inspect.cpuprofile'),
            JSON.stringify(msg.result.profile)
          )
          console.log('\n[profile saved]')
          try {
            const { execFileSync } = require('child_process')
            execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
          } catch {}
          process.exit(0)
        }
      })
    })
  })
}

let seq = 0
function send(method, params) {
  ws.send(JSON.stringify({ id: ++seq, method, params }))
}

let finished = false
function finish() {
  if (finished || !ws) return
  finished = true
  console.log('\n[saving profile...]')
  send('Profiler.stop')
}

setTimeout(() => {
  console.log('\nTIMEOUT')
  try {
    const { execFileSync } = require('child_process')
    execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore' })
  } catch {}
  process.exit(1)
}, 120000)
