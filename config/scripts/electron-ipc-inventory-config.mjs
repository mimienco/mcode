export const SOURCE_DIRECTORIES = ['src/main', 'src/preload', 'src/renderer/src', 'src/shared']

export const SOURCE_EXTENSIONS = new Set([
  '.ts',
  '.tsx',
  '.js',
  '.jsx',
  '.mjs',
  '.cjs',
  '.mts',
  '.cts'
])

export const EXCLUDED_DIRECTORIES = new Set([
  '__fixtures__',
  '__mocks__',
  '__snapshots__',
  '__tests__',
  'fixtures',
  'node_modules',
  'out',
  'target',
  'test',
  'tests'
])

export const TEST_FILE_PATTERN = /(?:^|[.-])(?:bench|fixture|harness|mock|spec|test)(?:[.-]|$)/i

export const IPC_MAIN_METHODS = new Map([
  ['handle', { role: 'registration', direction: 'renderer_to_main', transport: 'invoke' }],
  ['handleOnce', { role: 'registration', direction: 'renderer_to_main', transport: 'invoke_once' }],
  ['on', { role: 'registration', direction: 'renderer_to_main', transport: 'send' }],
  ['once', { role: 'registration', direction: 'renderer_to_main', transport: 'send_once' }]
])

export const IPC_RENDERER_METHODS = new Map([
  ['invoke', { role: 'caller', direction: 'renderer_to_main', transport: 'invoke' }],
  ['send', { role: 'caller', direction: 'renderer_to_main', transport: 'send' }],
  ['sendSync', { role: 'caller', direction: 'renderer_to_main', transport: 'send_sync' }],
  ['postMessage', { role: 'caller', direction: 'renderer_to_main', transport: 'post_message' }],
  ['on', { role: 'subscription', direction: 'main_to_renderer', transport: 'event' }],
  ['once', { role: 'subscription', direction: 'main_to_renderer', transport: 'event_once' }]
])
