import * as assert from 'assert'

import {
  buildBonsaiArgs,
  buildProjectMapText,
  buildStatusText,
  buildSuccessMessage,
  extractProjectMap,
  parseRunReport
} from './bonsai'

const args = buildBonsaiArgs('/repo', {
  binaryPath: '',
  exclude: ['**/generated/**'],
  include: ['src/**'],
  level: 2,
  maxTokens: 12000,
  outputFile: '/tmp/context.xml',
  outputFormat: 'json',
  respectGitignore: false
})

assert.deepStrictEqual(args, [
  '/repo',
  '--max-tokens',
  '12000',
  '--level',
  '2',
  '--format',
  'json',
  '--output',
  'file',
  '--output-file',
  '/tmp/context.xml',
  '--summary',
  '--stats',
  '--include',
  'src/**',
  '--exclude',
  '**/generated/**',
  '--no-respect-gitignore'
])

const changedArgs = buildBonsaiArgs('/repo', {
  binaryPath: '',
  exclude: [],
  include: [],
  level: 2,
  maxTokens: 12000,
  outputFile: '/tmp/context.xml',
  outputFormat: 'xml',
  respectGitignore: true
}, { incremental: true })

assert.ok(changedArgs.includes('--incremental'))
assert.ok(changedArgs.includes('--incremental-summary'))

const report = parseRunReport(`summary:
  output: /tmp/context.xml
  files_included: 3
  selected_level: 2
  output_tokens: 900 / 12000
stats:
  raw_tokens: 3000
  shrunk_tokens: 900
  tokens_saved: 2100
  saving_percent: 70.00
  files_scanned: 3
`)

assert.strictEqual(report.filesIncluded, 3)
assert.strictEqual(report.outputTokens, 900)
assert.strictEqual(report.outputTokensBudget, 12000)
assert.strictEqual(report.savingPercent, 70)
assert.strictEqual(buildStatusText(report), 'Bonsai: 900/12000 tokens, 3 files')

const xmlMap = extractProjectMap(
  `<project_map>
<entry path="src/main.rs" level="2" tokens="123" />
<entry path="src/&lt;bad&gt;.rs" level="3" tokens="5" />
</project_map>`,
  'xml'
)

assert.deepStrictEqual(xmlMap, [
  { path: 'src/main.rs', level: 2, tokens: 123 },
  { path: 'src/<bad>.rs', level: 3, tokens: 5 }
])

const jsonMap = extractProjectMap(
  JSON.stringify({ project_map: [{ path: 'src/main.rs', level: 2, tokens: 123 }] }),
  'json'
)

assert.deepStrictEqual(jsonMap, [{ path: 'src/main.rs', level: 2, tokens: 123 }])
assert.ok(buildProjectMapText(jsonMap).includes('src/main.rs'))
assert.ok(buildSuccessMessage('/tmp/context.xml', report, 'Done.').includes('saved 70.00%'))

console.log('bonsai extension smoke ok')
