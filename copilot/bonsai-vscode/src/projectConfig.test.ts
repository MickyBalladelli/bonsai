import * as assert from 'assert'
import * as fs from 'fs/promises'
import * as os from 'os'
import * as path from 'path'

import { readProjectConfig } from './projectConfig'

async function withFixture(run: (root: string) => Promise<void>): Promise<void> {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'bonsai-config-'))
  try {
    await run(root)
  } finally {
    await fs.rm(root, { force: true, recursive: true })
  }
}

async function verifyMissingFileYieldsDefaults(): Promise<void> {
  await withFixture(async root => {
    assert.deepStrictEqual(await readProjectConfig(root), {})
  })
}

async function verifyFullConfigParses(): Promise<void> {
  await withFixture(async root => {
    await fs.writeFile(
      path.join(root, '.bonsai.toml'),
      [
        '# Bonsai project settings.',
        'max_tokens = 8000',
        'level = 2',
        'format = "json"',
        'output_file = "out/bonsai.json"',
        'include = ["src/**", "lib/**"]',
        'exclude = []',
        'respect_gitignore = false',
        ''
      ].join('\n')
    )

    assert.deepStrictEqual(await readProjectConfig(root), {
      maxTokens: 8000,
      level: 2,
      outputFormat: 'json',
      outputFile: 'out/bonsai.json',
      include: ['src/**', 'lib/**'],
      exclude: [],
      respectGitignore: false
    })
  })
}

async function verifyCommentsAndQuoting(): Promise<void> {
  await withFixture(async root => {
    await fs.writeFile(
      path.join(root, '.bonsai.toml'),
      [
        "output_file = 'quoted.json' # trailing comment",
        'include = ["a#b", \'c#d\']',
        'unknown_future_key = 42',
        ''
      ].join('\n')
    )

    const config = await readProjectConfig(root)
    assert.strictEqual(config.outputFile, 'quoted.json')
    assert.deepStrictEqual(config.include, ['a#b', 'c#d'])
    assert.strictEqual('unknown_future_key' in config, false)
  })
}

async function verifyOutputFormatAlias(): Promise<void> {
  await withFixture(async root => {
    await fs.writeFile(path.join(root, '.bonsai.toml'), 'output_format = "xml"\n')
    assert.strictEqual((await readProjectConfig(root)).outputFormat, 'xml')
  })
}

async function verifyInvalidConfigsThrow(): Promise<void> {
  const cases: Array<[string, string]> = [
    ['no separator', 'max_tokens'],
    ['bad integer', 'max_tokens = many'],
    ['zero tokens', 'max_tokens = 0'],
    ['bad level', 'level = 5'],
    ['bad format', 'format = "yaml"'],
    ['bad boolean', 'respect_gitignore = yes'],
    ['unquoted string', 'output_file = bare.json'],
    ['bad array', 'include = src/**']
  ]
  for (const [name, contents] of cases) {
    await withFixture(async root => {
      await fs.writeFile(path.join(root, '.bonsai.toml'), `${contents}\n`)
      await assert.rejects(readProjectConfig(root), /\.bonsai\.toml/, name)
    })
  }
}

async function main(): Promise<void> {
  await verifyMissingFileYieldsDefaults()
  await verifyFullConfigParses()
  await verifyCommentsAndQuoting()
  await verifyOutputFormatAlias()
  await verifyInvalidConfigsThrow()
  console.log('project config ok')
}

void main().catch(error => {
  console.error(error)
  process.exitCode = 1
})
