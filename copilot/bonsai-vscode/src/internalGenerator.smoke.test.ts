import * as assert from 'assert'
import * as fs from 'fs/promises'
import * as os from 'os'
import * as path from 'path'

import { BonsaiConfig, ProjectMapEntry } from './bonsai'
import { generateRepository, InternalGenerationResult } from './internalGenerator'

function config(root: string): BonsaiConfig {
  return {
    exclude: [],
    include: [],
    level: 2,
    maxTokens: 12000,
    outputFile: path.join(root, 'bonsai.json'),
    outputFormat: 'json',
    respectGitignore: false
  }
}

async function write(root: string, relativePath: string, contents: string): Promise<void> {
  const target = path.join(root, relativePath)
  await fs.mkdir(path.dirname(target), { recursive: true })
  await fs.writeFile(target, contents)
}

async function withFixture(run: (root: string) => Promise<void>): Promise<void> {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'bonsai-vscode-'))
  try {
    await run(root)
  } finally {
    await fs.rm(root, { force: true, recursive: true })
  }
}

function entry(result: InternalGenerationResult, relativePath: string): ProjectMapEntry {
  const found = result.projectMap.find(candidate => candidate.path === relativePath)
  if (!found) {
    throw new Error(`Expected ${relativePath} in the project map.`)
  }
  return found
}

function contextContent(result: InternalGenerationResult, relativePath: string): string {
  const parsed = JSON.parse(result.contextText) as { files: Array<{ path: string; content: string }> }
  const found = parsed.files.find(file => file.path === relativePath)
  if (!found) {
    throw new Error(`Expected ${relativePath} in the context.`)
  }
  return found.content
}

async function verifyRustCrateDependencyAndComments(): Promise<void> {
  await withFixture(async root => {
    await write(root, 'crates/engine/src/lib.rs', `// comment that is not needed\nuse crate::billing::charge\n\npub fn run() {\n  charge()\n}\n`)
    await write(root, 'crates/engine/src/billing.rs', 'pub fn charge() {}\n')

    const result = await generateRepository(root, config(root), {
      focus: 'invoice request',
      filePriorities: [{ path: 'crates/engine/src/lib.rs', level: 1 }]
    })

    assert.strictEqual(entry(result, 'crates/engine/src/lib.rs').level, 1)
    assert.strictEqual(entry(result, 'crates/engine/src/billing.rs').level, 2)
    assert.strictEqual(entry(result, 'crates/engine/src/billing.rs').reason, 'related dependency or caller')
    assert.ok(!contextContent(result, 'crates/engine/src/lib.rs').includes('comment that is not needed'))

    const commentsKept = await generateRepository(root, config(root), {
      focus: 'invoice request',
      filePriorities: [{ path: 'crates/engine/src/lib.rs', level: 1, includeComments: true }]
    })

    assert.ok(contextContent(commentsKept, 'crates/engine/src/lib.rs').includes('comment that is not needed'))
  })
}

async function verifyPythonAndCDependencies(): Promise<void> {
  await withFixture(async root => {
    await write(root, 'pkg/main.py', 'from . import support\n\ndef run():\n    return support.value\n')
    await write(root, 'pkg/support.py', 'value = 1\n')
    await write(root, 'app/main.c', '#include "shared/value.h"\n\nint main(void) { return value; }\n')
    await write(root, 'include/shared/value.h', 'static const int value = 1;\n')

    const result = await generateRepository(root, config(root), {
      focus: 'invoice request',
      filePriorities: [
        { path: 'pkg/main.py', level: 1 },
        { path: 'app/main.c', level: 1 }
      ]
    })

    assert.strictEqual(entry(result, 'pkg/support.py').level, 2)
    assert.strictEqual(entry(result, 'include/shared/value.h').level, 2)
  })
}

async function verifyScopedTestMatchingAndSourceEvidence(): Promise<void> {
  await withFixture(async root => {
    await write(root, 'src/auth.ts', 'export function authenticate() { return true }\n')
    await write(root, 'src/auth.test.ts', 'describe("auth", () => {})\n')
    await write(root, 'other/auth.ts', 'export function authenticateElsewhere() { return false }\n')
    await write(root, 'src/worker.ts', "const code = 'invoice'\nexport { code }\n")

    const result = await generateRepository(root, config(root), {
      focus: 'invoice request',
      filePriorities: [{ path: 'src/auth.ts', level: 1 }]
    })

    assert.strictEqual(entry(result, 'src/auth.test.ts').level, 2)
    assert.strictEqual(entry(result, 'other/auth.ts').level, 3)
    assert.strictEqual(entry(result, 'src/worker.ts').level, 2)
  })
}

async function verifyFidelityWarningsForLossyOutput(): Promise<void> {
  await withFixture(async root => {
    await write(root, 'src/auth.ts', 'export function authenticate() { return true }\n')
    await write(root, 'src/worker.ts', "export function work() { return 'work' }\n")

    const limited = config(root)
    limited.level = 2
    limited.maxTokens = 30
    const result = await generateRepository(root, limited)

    assert.ok(result.warnings.length > 0)
    assert.ok(result.warnings.some(warning => warning.includes('tree-map summaries')))
    assert.ok(result.contextText.includes('"warnings"'))
    assert.ok(result.contextText.includes('must not be treated as evidence of behavior'))
  })
}

async function verifyBudgetLoopConvergesPastTwoHundredAttempts(): Promise<void> {
  await withFixture(async root => {
    for (let file = 0; file < 20; file += 1) {
      const name = `f${String(file).padStart(2, '0')}`
      const lines: string[] = []
      for (let fn = 0; fn < 24; fn += 1) {
        const fname = `${name}_${String(fn).padStart(2, '0')}`
        lines.push(`export function ${fname}(p01: string, p02: string, p03: string, p04: string): string { return p01 + p02 + p03 + p04 }`)
      }
      await write(root, `src/${name}.ts`, `${lines.join('\n')}\n`)
    }

    const limited = config(root)
    limited.level = 2
    limited.maxTokens = 3200
    const result = await generateRepository(root, limited)

    assert.ok(result.report.outputTokens !== undefined && result.report.outputTokens <= 3200)
    assert.ok(!result.warnings.some(warning => warning.includes('above max_tokens')))
    assert.ok(!result.contextText.includes('above max_tokens'))
  })
}

async function main(): Promise<void> {
  await verifyRustCrateDependencyAndComments()
  await verifyPythonAndCDependencies()
  await verifyScopedTestMatchingAndSourceEvidence()
  await verifyFidelityWarningsForLossyOutput()
  await verifyBudgetLoopConvergesPastTwoHundredAttempts()
  console.log('internal generator smoke ok')
}

void main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
