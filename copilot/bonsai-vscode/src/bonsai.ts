export type BonsaiConfig = {
  binaryPath: string
  exclude: string[]
  include: string[]
  level: number
  maxTokens: number
  outputFile: string
  outputFormat: 'json' | 'xml'
  respectGitignore: boolean
}

export type RunMode = {
  incremental?: boolean
}

export type RunReport = {
  filesIncluded?: number
  outputTokens?: number
  outputTokensBudget?: number
  rawTokens?: number
  savingPercent?: number
  shrunkTokens?: number
  tokensSaved?: number
}

export type ProjectMapEntry = {
  level: number
  path: string
  tokens: number
}

export function buildBonsaiArgs(workspaceRoot: string, config: BonsaiConfig, mode: RunMode = {}): string[] {
  const args = [
    workspaceRoot,
    '--max-tokens',
    String(config.maxTokens),
    '--level',
    String(config.level),
    '--format',
    config.outputFormat,
    '--output',
    'file',
    '--output-file',
    config.outputFile,
    '--summary',
    '--stats'
  ]

  if (mode.incremental) {
    args.push('--incremental', '--incremental-summary')
  }

  for (const pattern of config.include) {
    args.push('--include', pattern)
  }

  for (const pattern of config.exclude) {
    args.push('--exclude', pattern)
  }

  if (!config.respectGitignore) {
    args.push('--no-respect-gitignore')
  }

  return args
}

export function parseRunReport(stdout: string): RunReport {
  return {
    filesIncluded: readNumber(stdout, /^  files_included: (\d+)$/m),
    outputTokens: readNumber(stdout, /^  output_tokens: (\d+) \/ \d+$/m),
    outputTokensBudget: readNumber(stdout, /^  output_tokens: \d+ \/ (\d+)$/m),
    rawTokens: readNumber(stdout, /^  raw_tokens: (\d+)$/m),
    shrunkTokens: readNumber(stdout, /^  shrunk_tokens: (\d+)$/m),
    tokensSaved: readNumber(stdout, /^  tokens_saved: (\d+)$/m),
    savingPercent: readFloat(stdout, /^  saving_percent: ([\d.]+)$/m)
  }
}

export function extractProjectMap(contextText: string, outputFormat: 'json' | 'xml'): ProjectMapEntry[] {
  if (outputFormat === 'json') {
    const parsed = JSON.parse(contextText) as { project_map?: ProjectMapEntry[] }
    return parsed.project_map ?? []
  }

  const entries: ProjectMapEntry[] = []
  const pattern = /<entry path="([^"]+)" level="(\d+)" tokens="(\d+)" \/>/g
  let match: RegExpExecArray | null
  while ((match = pattern.exec(contextText)) !== null) {
    entries.push({
      path: decodeXml(match[1]),
      level: Number(match[2]),
      tokens: Number(match[3])
    })
  }

  return entries
}

export function buildProjectMapText(entries: ProjectMapEntry[]): string {
  return entries
    .map(entry => `${entry.tokens.toString().padStart(5, ' ')} tokens  L${entry.level}  ${entry.path}`)
    .join('\n')
}

export function buildContextPrompt(outputFile: string, contextText?: string): string {
  if (contextText) {
    return `Use this Bonsai context as compressed repository context for Copilot Chat, ChatGPT, or Codex in VS Code, then answer my next question.\n\n${contextText}`
  }

  return `Use the Bonsai context opened at ${outputFile} as compressed repository context for Copilot Chat, ChatGPT, or Codex in VS Code, then answer my next question.`
}

export function buildSuccessMessage(outputFile: string, report: RunReport, nextStep: string): string {
  const tokenText = report.shrunkTokens !== undefined
    ? `${report.shrunkTokens}${report.outputTokensBudget !== undefined ? ` / ${report.outputTokensBudget}` : ''} tokens`
    : 'token count unavailable'
  const savedText = report.savingPercent !== undefined
    ? `, saved ${report.savingPercent.toFixed(2)}%`
    : ''
  const fileText = report.filesIncluded !== undefined
    ? `, ${report.filesIncluded} files`
    : ''

  return `Bonsai wrote ${outputFile} (${tokenText}${savedText}${fileText}). ${nextStep}`
}

export function buildStatusText(report: RunReport): string {
  const tokenText = report.shrunkTokens !== undefined
    ? `${report.shrunkTokens}${report.outputTokensBudget !== undefined ? `/${report.outputTokensBudget}` : ''} tokens`
    : 'tokens unknown'
  const fileText = report.filesIncluded !== undefined
    ? `${report.filesIncluded} files`
    : 'files unknown'

  return `Bonsai: ${tokenText}, ${fileText}`
}

function readNumber(text: string, pattern: RegExp): number | undefined {
  const value = text.match(pattern)?.[1]
  return value === undefined ? undefined : Number(value)
}

function readFloat(text: string, pattern: RegExp): number | undefined {
  const value = text.match(pattern)?.[1]
  return value === undefined ? undefined : Number.parseFloat(value)
}

function decodeXml(value: string): string {
  return value
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&gt;/g, '>')
    .replace(/&lt;/g, '<')
    .replace(/&amp;/g, '&')
}
