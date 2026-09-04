import * as fs from 'fs/promises'
import * as path from 'path'

export type BonsaiProjectConfig = {
  maxTokens?: number
  level?: number
  outputFile?: string
  outputFormat?: 'xml' | 'json'
  include?: string[]
  exclude?: string[]
  respectGitignore?: boolean
}

const CONFIG_FILE_NAME = '.bonsai.toml'

export async function readProjectConfig(root: string): Promise<BonsaiProjectConfig> {
  const configPath = path.join(root, CONFIG_FILE_NAME)
  let contents: string
  try {
    contents = await fs.readFile(configPath, 'utf8')
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
      return {}
    }
    throw new Error(`Cannot read ${CONFIG_FILE_NAME}: ${String(error)}`)
  }

  return parseProjectConfig(contents, configPath)
}

function parseProjectConfig(contents: string, configPath: string): BonsaiProjectConfig {
  const config: BonsaiProjectConfig = {}
  for (const [index, rawLine] of contents.split(/\r?\n/).entries()) {
    const lineNumber = index + 1
    const line = stripComment(rawLine).trim()
    if (!line) {
      continue
    }

    const separator = line.indexOf('=')
    if (separator < 0) {
      throw new Error(`Invalid ${CONFIG_FILE_NAME}:${lineNumber}: expected key = value`)
    }

    const key = line.slice(0, separator).trim()
    const value = line.slice(separator + 1).trim()
    try {
      switch (key) {
        case 'max_tokens':
          config.maxTokens = parseInteger(value)
          break
        case 'level':
          config.level = parseInteger(value)
          break
        case 'output_file':
          config.outputFile = parseString(value)
          break
        case 'format':
        case 'output_format': {
          const format = parseString(value)
          if (format !== 'xml' && format !== 'json') {
            throw new Error('must be xml or json')
          }
          config.outputFormat = format
          break
        }
        case 'include':
          config.include = parseStringArray(value)
          break
        case 'exclude':
          config.exclude = parseStringArray(value)
          break
        case 'respect_gitignore':
          config.respectGitignore = parseBoolean(value)
          break
        default:
          break
      }
    } catch (error) {
      throw new Error(`Invalid ${CONFIG_FILE_NAME}:${lineNumber} for ${key}: ${String(error)}`)
    }
  }

  if (config.maxTokens !== undefined && config.maxTokens < 1) {
    throw new Error(`Invalid ${configPath}: max_tokens must be greater than zero`)
  }
  if (config.level !== undefined && ![1, 2, 3].includes(config.level)) {
    throw new Error(`Invalid ${configPath}: level must be 1, 2, or 3`)
  }
  return config
}

function stripComment(line: string): string {
  let quote: '"' | "'" | undefined
  let escaped = false
  for (let index = 0; index < line.length; index += 1) {
    const character = line[index]
    if (quote === '"' && character === '\\' && !escaped) {
      escaped = true
      continue
    }
    if (quote && character === quote && !escaped) {
      quote = undefined
    } else if (!quote && (character === '"' || character === "'")) {
      quote = character
    }
    escaped = false
    if (!quote && character === '#') {
      return line.slice(0, index)
    }
  }
  return line
}

function parseInteger(value: string): number {
  if (!/^\d+$/.test(value)) {
    throw new Error('must be a positive integer')
  }
  const parsed = Number(value)
  if (!Number.isSafeInteger(parsed)) {
    throw new Error('is too large')
  }
  return parsed
}

function parseBoolean(value: string): boolean {
  if (value === 'true') {
    return true
  }
  if (value === 'false') {
    return false
  }
  throw new Error('must be true or false')
}

function parseString(value: string): string {
  if (value.startsWith('"') && value.endsWith('"')) {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed === 'string') {
      return parsed
    }
  }
  if (value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1)
  }
  throw new Error('must be a quoted string')
}

function parseStringArray(value: string): string[] {
  if (!value.startsWith('[') || !value.endsWith(']')) {
    throw new Error('must be an array of quoted strings')
  }
  const contents = value.slice(1, -1).trim()
  if (!contents) {
    return []
  }

  return splitArrayItems(contents).map(item => parseString(item.trim()))
}

function splitArrayItems(value: string): string[] {
  const items: string[] = []
  let start = 0
  let quote: '"' | "'" | undefined
  let escaped = false
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index]
    if (quote === '"' && character === '\\' && !escaped) {
      escaped = true
      continue
    }
    if (quote && character === quote && !escaped) {
      quote = undefined
    } else if (!quote && (character === '"' || character === "'")) {
      quote = character
    } else if (!quote && character === ',') {
      items.push(value.slice(start, index))
      start = index + 1
    }
    escaped = false
  }
  items.push(value.slice(start))
  return items
}
