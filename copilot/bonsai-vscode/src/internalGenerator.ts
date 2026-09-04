import { createHash } from 'crypto'
import * as fs from 'fs/promises'
import * as path from 'path'

import { BonsaiConfig, ProjectMapEntry, RunReport } from './bonsai'

const DEFAULT_MAX_FILE_BYTES = 1_048_576
const MAX_CONTEXT_FILE_BYTES = 10_000_000
const IMPORT_BLOCK_KEEP = 5
const MAX_TREE_LINES = 120
const MAX_TEXT_LINES = 180
const ALWAYS_IGNORED_DIRECTORIES = new Set([
  '.git',
  'node_modules',
  'target',
  '.venv',
  'venv',
  '__pycache__',
  '.pytest_cache',
  '.mypy_cache',
  '.tox',
  '.gradle',
  '.build',
  'build',
  'dist',
  'coverage'
])
const SUPPORTED_EXTENSIONS = new Set([
  'js', 'jsx', 'ts', 'tsx', 'py', 'rs', 'go', 'java', 'cs', 'swift', 'kt',
  'c', 'h', 'cpp', 'hpp', 'm', 'mm', 'vue', 'svelte', 'astro', 'html',
  'md', 'json', 'yaml', 'yml', 'toml'
])

type CompressionLevel = 1 | 2 | 3

type FileCandidate = {
  absolutePath: string
  relativePath: string
  size: number
  modifiedMs: number
  signature: string
}

type FileVariants = {
  full: string
  skeleton: string
  treeMap: string
}

type WorkingFile = {
  path: string
  rawTokenCount: number
  variants: FileVariants
  level: CompressionLevel
  tokenCount: number
  contentHash: string
}

type IgnoreRule = {
  base: string
  pattern: string
  negated: boolean
}

export type InternalGenerationOptions = {
  incremental?: boolean
  previousSignatures?: Record<string, string>
}

export type InternalGenerationResult = {
  contextText: string
  contextFiles: Array<{ outputFile: string; contextText: string }>
  projectMap: ProjectMapEntry[]
  repositoryUrl?: string
  report: RunReport
  signatures: Record<string, string>
}

export async function generateRepository(
  root: string,
  config: BonsaiConfig,
  options: InternalGenerationOptions = {}
): Promise<InternalGenerationResult> {
  const candidates = await collectFiles(root, config)
  if (candidates.length === 0) {
    throw new Error('No supported source files were found in this workspace.')
  }

  const signatures = Object.fromEntries(
    candidates.map(candidate => [candidate.relativePath, candidate.signature])
  )
  const previousSignatures = options.previousSignatures
  const hasBaseline = Boolean(previousSignatures && Object.keys(previousSignatures).length > 0)
  const selectedCandidates = options.incremental && hasBaseline
    ? candidates.filter(candidate => previousSignatures?.[candidate.relativePath] !== candidate.signature)
    : candidates
  const deletedFiles = options.incremental && hasBaseline
    ? Object.keys(previousSignatures ?? {}).filter(relativePath => !signatures[relativePath]).sort()
    : []

  if (selectedCandidates.length === 0 && deletedFiles.length === 0) {
    throw new Error('No files changed since the previous Bonsai run.')
  }

  const requestedLevel = normalizeLevel(config.level)
  const files = await Promise.all(selectedCandidates.map(async candidate => {
    const source = await fs.readFile(candidate.absolutePath, 'utf8')
    const variants = buildVariants(candidate.relativePath, source)
    return {
      path: candidate.relativePath,
      rawTokenCount: estimateTokens(source),
      variants,
      level: requestedLevel,
      tokenCount: estimateTokens(contentForLevel(variants, requestedLevel)),
      contentHash: createHash('sha256').update(source).digest('hex')
    } satisfies WorkingFile
  }))

  const metadata = {
    generatedAt: new Date().toISOString(),
    repoRoot: root,
    maxTokens: Math.max(1, Math.floor(config.maxTokens)),
    compressionLevel: requestedLevel,
    fileCount: files.length
  }
  const rawFiles = files.map(file => ({
    ...file,
    level: 1 as CompressionLevel,
    tokenCount: estimateTokens(file.variants.full)
  }))
  const rawContext = formatContext(rawFiles, metadata, config.outputFormat, deletedFiles)
  const rawTokens = estimateTokens(rawContext)
  const optimizedFiles = files.map(file => ({ ...file, variants: { ...file.variants } }))
  const { contextText: fullContextText, outputTokens } = fitBudget(
    optimizedFiles,
    metadata,
    config,
    deletedFiles
  )
  const contextFiles = splitContextFiles(
    optimizedFiles,
    metadata,
    config.outputFormat,
    deletedFiles,
    config.outputFile
  )
  const projectMap = optimizedFiles.map(file => ({
    path: file.path,
    level: file.level,
    tokens: file.tokenCount,
    savedPercent: calculateSavingPercent(file.rawTokenCount, file.tokenCount)
  }))
  const tokensSaved = Math.max(0, rawTokens - outputTokens)

  return {
    contextText: contextFiles[0].contextText,
    contextFiles,
    projectMap,
    repositoryUrl: await readRepositoryUrl(root),
    signatures,
    report: {
      filesIncluded: optimizedFiles.length,
      outputTokens,
      outputTokensBudget: metadata.maxTokens,
      rawTokens,
      shrunkTokens: outputTokens,
      tokensSaved,
      savingPercent: rawTokens === 0 ? 0 : tokensSaved / rawTokens * 100
    }
  }
}

async function readRepositoryUrl(root: string): Promise<string | undefined> {
  let gitPath = path.join(root, '.git')
  try {
    const gitStat = await fs.stat(gitPath)
    if (gitStat.isFile()) {
      const gitPointer = await fs.readFile(gitPath, 'utf8')
      const match = gitPointer.match(/^gitdir:\s*(.+)$/m)
      if (!match) {
        return undefined
      }
      gitPath = path.resolve(root, match[1].trim())
    }

    const gitConfig = await fs.readFile(path.join(gitPath, 'config'), 'utf8')
    const remoteSections = [...gitConfig.matchAll(/\[remote\s+"([^"]+)"\]([\s\S]*?)(?=\n\[|$)/g)]
    const origin = remoteSections.find(section => section[1] === 'origin')
    const remote = origin ?? remoteSections[0]
    const rawUrl = remote?.[2].match(/^\s*url\s*=\s*(.+)$/m)?.[1].trim()
    return rawUrl ? normalizeRepositoryUrl(rawUrl) : undefined
  } catch {
    return undefined
  }
}

function normalizeRepositoryUrl(value: string): string | undefined {
  if (/^https?:\/\//i.test(value)) {
    return value.replace(/\.git$/, '')
  }

  const scpMatch = value.match(/^git@([^:]+):(.+)$/i)
  if (scpMatch) {
    return `https://${scpMatch[1]}/${scpMatch[2].replace(/\.git$/, '')}`
  }

  if (/^ssh:\/\//i.test(value)) {
    try {
      const parsed = new URL(value)
      return `https://${parsed.hostname}${parsed.pathname.replace(/\.git$/, '')}`
    } catch {
      return undefined
    }
  }

  return undefined
}

async function collectFiles(root: string, config: BonsaiConfig): Promise<FileCandidate[]> {
  const files: FileCandidate[] = []

  async function visit(directory: string, inheritedRules: IgnoreRule[]): Promise<void> {
    const rules = config.respectGitignore
      ? [...inheritedRules, ...await loadIgnoreRules(directory, root)]
      : inheritedRules
    const entries = await fs.readdir(directory, { withFileTypes: true })
    for (const entry of entries) {
      const absolutePath = path.join(directory, entry.name)
      const relativePath = toRelativePath(root, absolutePath)

      if (entry.isDirectory()) {
        if (ALWAYS_IGNORED_DIRECTORIES.has(entry.name) || isIgnored(relativePath, true, rules)) {
          continue
        }
        await visit(absolutePath, rules)
        continue
      }

      if (!entry.isFile() || !isSupportedFile(relativePath) || isIgnored(relativePath, false, rules)) {
        continue
      }
      if (!matchesFilters(relativePath, config.include, config.exclude)) {
        continue
      }

      const metadata = await fs.stat(absolutePath)
      if (metadata.size > DEFAULT_MAX_FILE_BYTES) {
        continue
      }

      files.push({
        absolutePath,
        relativePath,
        size: metadata.size,
        modifiedMs: metadata.mtimeMs,
        signature: `${metadata.size}:${metadata.mtimeMs}`
      })
    }
  }

  await visit(root, [])
  files.sort((left, right) => left.relativePath.localeCompare(right.relativePath))
  return files
}

async function loadIgnoreRules(directory: string, root: string): Promise<IgnoreRule[]> {
  const rules: IgnoreRule[] = []
  for (const name of ['.gitignore', '.cursorignore']) {
    try {
      const contents = await fs.readFile(path.join(directory, name), 'utf8')
      for (const rawLine of contents.split(/\r?\n/)) {
        const line = rawLine.trim()
        if (!line || line.startsWith('#')) {
          continue
        }
        const negated = line.startsWith('!')
        const pattern = normalizeIgnorePattern(negated ? line.slice(1) : line)
        if (pattern) {
          rules.push({
            base: toRelativePath(root, directory),
            pattern,
            negated
          })
        }
      }
    } catch {
      continue
    }
  }
  return rules
}

function normalizeIgnorePattern(value: string): string {
  let pattern = value.replace(/\\/g, '/').trim()
  if (pattern.startsWith('/')) {
    pattern = pattern.slice(1)
  }
  if (pattern.endsWith('/')) {
    pattern += '**'
  }
  return pattern
}

function isIgnored(relativePath: string, directory: boolean, rules: IgnoreRule[]): boolean {
  let ignored = false
  for (const rule of rules) {
    const relativeToBase = path.posix.relative(rule.base || '.', relativePath)
    if (relativeToBase === '..' || relativeToBase.startsWith('../')) {
      continue
    }
    const candidates = directory ? [relativeToBase, `${relativeToBase}/`] : [relativeToBase]
    if (candidates.some(candidate => matchesGlob(rule.pattern, candidate))) {
      ignored = !rule.negated
    }
  }
  return ignored
}

function matchesFilters(relativePath: string, include: string[], exclude: string[]): boolean {
  const included = include.length === 0 || include.some(pattern => matchesGlob(pattern, relativePath))
  const excluded = exclude.some(pattern => matchesGlob(pattern, relativePath))
  return included && !excluded
}

function matchesGlob(pattern: string, value: string): boolean {
  const normalizedPattern = pattern.replace(/\\/g, '/').replace(/^\.\//, '')
  const normalizedValue = value.replace(/\\/g, '/')
  const regex = globRegex(normalizedPattern)
  if (regex.test(normalizedValue)) {
    return true
  }
  if (!normalizedPattern.includes('/')) {
    return regex.test(path.posix.basename(normalizedValue))
  }
  return false
}

function globRegex(pattern: string): RegExp {
  let source = '^'
  for (let index = 0; index < pattern.length; index += 1) {
    const character = pattern[index]
    if (character === '*' && pattern[index + 1] === '*') {
      if (pattern[index + 2] === '/') {
        source += '(?:.*/)?'
        index += 2
      } else {
        source += '.*'
        index += 1
      }
      continue
    }
    if (character === '*') {
      source += '[^/]*'
      continue
    }
    if (character === '?') {
      source += '[^/]'
      continue
    }
    source += escapeRegexCharacter(character)
  }
  return new RegExp(`${source}$`)
}

function escapeRegexCharacter(value: string): string {
  return /[\\^$+?.()|{}[\]]/.test(value) ? `\\${value}` : value
}

function isSupportedFile(relativePath: string): boolean {
  const extension = path.posix.extname(relativePath).slice(1).toLowerCase()
  return SUPPORTED_EXTENSIONS.has(extension)
}

function toRelativePath(root: string, value: string): string {
  return path.relative(root, value).split(path.sep).join('/')
}

function buildVariants(relativePath: string, source: string): FileVariants {
  const extension = path.posix.extname(relativePath).slice(1).toLowerCase()
  const skeleton = isPlainText(extension)
    ? compactTextContext(relativePath, source)
    : stripCallableBodies(source, extension)
  const treeMap = isPlainText(extension)
    ? buildTextTreeMap(relativePath, source)
    : buildCodeTreeMap(source, extension)

  return {
    full: source,
    skeleton: collapseImportBlocks(skeleton, extension),
    treeMap: collapseImportBlocks(treeMap, extension)
  }
}

function normalizeLevel(value: number): CompressionLevel {
  if (value <= 1) {
    return 1
  }
  if (value >= 3) {
    return 3
  }
  return 2
}

function contentForLevel(variants: FileVariants, level: CompressionLevel): string {
  if (level === 1) {
    return variants.full
  }
  return level === 2 ? variants.skeleton : variants.treeMap
}

function fitBudget(
  files: WorkingFile[],
  metadata: { generatedAt: string; repoRoot: string; maxTokens: number; compressionLevel: number; fileCount: number },
  config: BonsaiConfig,
  deletedFiles: string[]
): { contextText: string; outputTokens: number } {
  let contextText = ''
  let outputTokens = 0

  for (let attempt = 0; attempt < 200; attempt += 1) {
    for (const file of files) {
      file.tokenCount = estimateTokens(contentForLevel(file.variants, file.level))
    }
    contextText = formatContext(files, metadata, config.outputFormat, deletedFiles)
    outputTokens = estimateTokens(contextText)
    if (outputTokens <= metadata.maxTokens) {
      return { contextText, outputTokens }
    }

    const downgrade = files
      .filter(file => file.level < 3)
      .sort((left, right) => right.tokenCount - left.tokenCount)[0]
    if (downgrade) {
      downgrade.level = (downgrade.level + 1) as CompressionLevel
      continue
    }

    const largest = files.slice().sort((left, right) => right.tokenCount - left.tokenCount)[0]
    if (!largest || largest.tokenCount <= 8) {
      break
    }
    const target = Math.max(8, Math.floor(largest.tokenCount * 0.8))
    const content = contentForLevel(largest.variants, largest.level)
    const truncated = truncateText(content, target)
    if (truncated === content) {
      break
    }
    if (largest.level === 1) {
      largest.variants.full = truncated
    } else if (largest.level === 2) {
      largest.variants.skeleton = truncated
    } else {
      largest.variants.treeMap = truncated
    }
  }

  return { contextText, outputTokens }
}

function splitContextFiles(
  files: WorkingFile[],
  metadata: { generatedAt: string; repoRoot: string; maxTokens: number; compressionLevel: number; fileCount: number },
  outputFormat: 'xml' | 'json',
  deletedFiles: string[],
  outputFile: string
): Array<{ outputFile: string; contextText: string }> {
  const chunks: WorkingFile[][] = []
  let current: WorkingFile[] = []

  for (const file of files) {
    const candidate = [...current, file]
    const candidateDeleted = chunks.length === 0 ? deletedFiles : []
    const candidateText = formatContext(candidate, metadata, outputFormat, candidateDeleted)
    if (current.length > 0 && Buffer.byteLength(candidateText, 'utf8') > MAX_CONTEXT_FILE_BYTES) {
      chunks.push(current)
      current = [file]
      continue
    }
    current = candidate
  }
  if (current.length > 0 || chunks.length === 0) {
    chunks.push(current)
  }

  const contextFiles: Array<{ outputFile: string; contextText: string }> = []
  for (const [index, chunk] of chunks.entries()) {
    const chunkDeleted = index === 0 ? deletedFiles : []
    const contextText = fitChunkByteLimit(chunk, metadata, outputFormat, chunkDeleted)
    contextFiles.push({
      outputFile: chunkOutputPath(outputFile, index),
      contextText
    })
  }
  return contextFiles
}

function fitChunkByteLimit(
  files: WorkingFile[],
  metadata: { generatedAt: string; repoRoot: string; maxTokens: number; compressionLevel: number; fileCount: number },
  outputFormat: 'xml' | 'json',
  deletedFiles: string[]
): string {
  let contextText = formatContext(files, metadata, outputFormat, deletedFiles)
  for (let attempt = 0; attempt < 100 && Buffer.byteLength(contextText, 'utf8') > MAX_CONTEXT_FILE_BYTES; attempt += 1) {
    const largest = files.slice().sort((left, right) => right.tokenCount - left.tokenCount)[0]
    if (!largest || largest.tokenCount <= 8) {
      break
    }
    const content = contentForLevel(largest.variants, largest.level)
    const truncated = truncateText(content, Math.max(8, Math.floor(largest.tokenCount * 0.75)))
    if (truncated === content) {
      break
    }
    if (largest.level === 1) {
      largest.variants.full = truncated
    } else if (largest.level === 2) {
      largest.variants.skeleton = truncated
    } else {
      largest.variants.treeMap = truncated
    }
    largest.tokenCount = estimateTokens(truncated)
    contextText = formatContext(files, metadata, outputFormat, deletedFiles)
  }
  return contextText
}

function chunkOutputPath(outputFile: string, index: number): string {
  if (index === 0) {
    return outputFile
  }
  const extension = path.extname(outputFile)
  const base = extension ? outputFile.slice(0, -extension.length) : outputFile
  return `${base}-${index + 1}${extension}`
}

function truncateText(text: string, maxTokens: number): string {
  if (estimateTokens(text) <= maxTokens) {
    return text
  }

  let low = 0
  let high = text.length
  while (low < high) {
    const middle = Math.ceil((low + high) / 2)
    const candidate = `${text.slice(0, middle).trimEnd()}...`
    if (estimateTokens(candidate) <= maxTokens) {
      low = middle
    } else {
      high = middle - 1
    }
  }

  const result = text.slice(0, low).trimEnd()
  return result ? `${result}...` : '...'
}

function estimateTokens(text: string): number {
  if (!text.trim()) {
    return 0
  }
  return Math.max(1, Math.ceil(text.trim().length / 4))
}

function calculateSavingPercent(rawTokens: number, compressedTokens: number): number {
  if (rawTokens === 0) {
    return 0
  }
  return Math.max(0, (rawTokens - compressedTokens) / rawTokens * 100)
}

function stripCallableBodies(source: string, extension: string): string {
  if (extension === 'py') {
    return stripPythonBodies(source)
  }

  const replacements: Array<{ start: number; end: number }> = []
  let index = 0
  while (index < source.length) {
    if (source[index] === '{') {
      const lineStart = source.lastIndexOf('\n', index - 1) + 1
      const prefix = source.slice(lineStart, index).trim()
      if (isCallablePrefix(prefix, extension)) {
        const end = findMatchingBrace(source, index)
        if (end !== undefined) {
          replacements.push({ start: index, end: end + 1 })
          index = end + 1
          continue
        }
      }
    }
    index += 1
  }

  if (replacements.length === 0) {
    return source
  }

  let output = ''
  let cursor = 0
  for (const replacement of replacements) {
    if (replacement.start < cursor) {
      continue
    }
    output += source.slice(cursor, replacement.start)
    output += '{ ... }'
    cursor = replacement.end
  }
  return output + source.slice(cursor)
}

function stripPythonBodies(source: string): string {
  const lines = source.split(/\r?\n/)
  const output: string[] = []
  let index = 0
  while (index < lines.length) {
    const line = lines[index]
    output.push(line)
    const definition = line.match(/^(\s*)(?:async\s+)?def\s+[^:]+:\s*(?:#.*)?$/)
    if (!definition) {
      index += 1
      continue
    }

    const baseIndent = definition[1].length
    let next = index + 1
    while (next < lines.length) {
      const candidate = lines[next]
      const trimmed = candidate.trim()
      if (!trimmed) {
        next += 1
        continue
      }
      if (candidate.search(/\S|$/) <= baseIndent) {
        break
      }
      next += 1
    }
    if (next > index + 1) {
      output.push(`${definition[1]}    ...`)
      index = next
    } else {
      index += 1
    }
  }
  return output.join('\n')
}

function isCallablePrefix(prefix: string, extension: string): boolean {
  const normalized = prefix.replace(/\/\/.*$/, '').trim()
  if (!normalized || /^(if|for|while|switch|catch|try|else|do)\b/.test(normalized)) {
    return false
  }
  if (/\b(class|interface|enum|namespace|struct|trait|impl)\b/.test(normalized)) {
    return false
  }
  if (extension === 'js' || extension === 'jsx' || extension === 'ts' || extension === 'tsx') {
    return /\bfunction\b|=>\s*$|\)\s*(?::[^{}]+)?$|\bconstructor\s*\(/.test(normalized)
  }
  if (extension === 'rs' || extension === 'go') {
    return /\b(fn|func)\b|\)\s*(?:const|throws|where|[A-Za-z_][\w<>]*)?\s*$/.test(normalized)
  }
  return /\b(def|function|constructor|init)\b|\)\s*(?::|->|throws|const)?\s*$/.test(normalized)
}

function findMatchingBrace(source: string, start: number): number | undefined {
  let depth = 0
  let quote = ''
  let lineComment = false
  let blockComment = false

  for (let index = start; index < source.length; index += 1) {
    const character = source[index]
    const next = source[index + 1]
    if (lineComment) {
      if (character === '\n') {
        lineComment = false
      }
      continue
    }
    if (blockComment) {
      if (character === '*' && next === '/') {
        blockComment = false
        index += 1
      }
      continue
    }
    if (quote) {
      if (character === '\\') {
        index += 1
      } else if (character === quote) {
        quote = ''
      }
      continue
    }
    if ((character === '/' && next === '/') || (character === '#' && index === source.lastIndexOf('\n', index - 1) + 1)) {
      lineComment = true
      continue
    }
    if (character === '/' && next === '*') {
      blockComment = true
      index += 1
      continue
    }
    if (character === '"' || character === "'" || character === '`') {
      quote = character
      continue
    }
    if (character === '{') {
      depth += 1
    } else if (character === '}') {
      depth -= 1
      if (depth === 0) {
        return index
      }
    }
  }
  return undefined
}

function buildCodeTreeMap(source: string, extension: string): string {
  const selected = source
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(line => line && (isImportLikeLine(line, extension) || isDeclarationLine(line)))
    .map(line => line.length > 180 ? `${line.slice(0, 177).trimEnd()}...` : line)
  return selected.slice(0, MAX_TREE_LINES).join('\n') || compactNonEmptyLines(source, MAX_TREE_LINES)
}

function buildTextTreeMap(relativePath: string, source: string): string {
  const extension = path.posix.extname(relativePath).slice(1).toLowerCase()
  const selected = source
    .split(/\r?\n/)
    .map(line => line.trim())
    .filter(line => line && (isImportantTextLine(line, extension) || isImportLikeLine(line, extension)))
    .slice(0, MAX_TREE_LINES)
  return selected.join('\n') || compactNonEmptyLines(source, MAX_TREE_LINES)
}

function compactTextContext(relativePath: string, source: string): string {
  const extension = path.posix.extname(relativePath).slice(1).toLowerCase()
  const lines = source.split(/\r?\n/)
  if (extension === 'md') {
    return compactMarkdown(lines)
  }
  if (extension === 'json' || extension === 'yaml' || extension === 'yml' || extension === 'toml') {
    return lines
      .filter(line => line.trim())
      .slice(0, MAX_TEXT_LINES)
      .join('\n')
  }
  return compactNonEmptyLines(source, MAX_TEXT_LINES)
}

function compactMarkdown(lines: string[]): string {
  const output: string[] = []
  let inFence = false
  for (const line of lines) {
    const trimmed = line.trim()
    if (trimmed.startsWith('```')) {
      inFence = !inFence
      output.push(line)
      continue
    }
    if (inFence || /^#{1,6}\s|^[-*+]\s|^\d+[.)]\s|^>\s|^\|/.test(trimmed)) {
      output.push(line)
    }
    if (output.length >= MAX_TEXT_LINES) {
      break
    }
  }
  return output.join('\n') || compactNonEmptyLines(lines.join('\n'), MAX_TEXT_LINES)
}

function compactNonEmptyLines(source: string, limit: number): string {
  return source
    .split(/\r?\n/)
    .filter(line => line.trim())
    .slice(0, limit)
    .join('\n')
}

function isPlainText(extension: string): boolean {
  return ['vue', 'svelte', 'astro', 'html', 'md', 'json', 'yaml', 'yml', 'toml', 'm', 'mm'].includes(extension)
}

function isImportLikeLine(line: string, extension: string): boolean {
  const trimmed = line.trim()
  if (['js', 'jsx', 'ts', 'tsx', 'py'].includes(extension)) {
    return trimmed.startsWith('import ') || trimmed.startsWith('from ')
  }
  if (extension === 'rs') {
    return trimmed.startsWith('use ')
  }
  if (extension === 'go') {
    return trimmed.startsWith('import ')
  }
  if (['java', 'swift', 'kt'].includes(extension)) {
    return trimmed.startsWith('import ')
  }
  if (extension === 'cs') {
    return trimmed.startsWith('using ')
  }
  return trimmed.startsWith('#include ') || trimmed.startsWith('#import ')
}

function collapseImportBlocks(source: string, extension: string): string {
  const lines = source.split(/\r?\n/)
  const output: string[] = []
  let index = 0
  while (index < lines.length) {
    if (extension === 'go' && lines[index].trim() === 'import (') {
      let end = index + 1
      while (end < lines.length && lines[end].trim() !== ')') {
        end += 1
      }
      const imports = lines.slice(index + 1, end).filter(line => line.trim())
      if (imports.length > IMPORT_BLOCK_KEEP && end < lines.length) {
        output.push(lines[index], ...imports.slice(0, IMPORT_BLOCK_KEEP))
        output.push(`    ... ${imports.length - IMPORT_BLOCK_KEEP} more imports`, lines[end])
        index = end + 1
        continue
      }
    }

    if (!isImportLikeLine(lines[index], extension)) {
      output.push(lines[index])
      index += 1
      continue
    }

    const start = index
    while (index < lines.length && isImportLikeLine(lines[index], extension)) {
      index += 1
    }
    const imports = lines.slice(start, index)
    if (imports.length <= IMPORT_BLOCK_KEEP) {
      output.push(...imports)
    } else {
      output.push(...imports.slice(0, IMPORT_BLOCK_KEEP), `... ${imports.length - IMPORT_BLOCK_KEEP} more imports`)
    }
  }

  return output.join('\n')
}

function isDeclarationLine(line: string): boolean {
  return /^(?:(?:export|public|private|protected|internal|static|async|abstract|final|override|virtual|sealed|default)\s+)*(?:function|class|interface|type|enum|namespace|struct|trait|impl|fn|def|func)\b/.test(line)
    || /^(?:export\s+)?(?:const|let|var)\s+[A-Za-z_$][\w$]*/.test(line)
    || /^(?:public|private|protected|static|async|override|virtual|final)\s+[^;{}]+\([^;{}]*\)/.test(line)
    || /^[A-Za-z_$][\w$<>\[\], ]*\s+[A-Za-z_$][\w$]*\s*\([^;{}]*\)/.test(line)
}

function isImportantTextLine(line: string, extension: string): boolean {
  if (extension === 'md') {
    return /^#{1,6}\s|^[-*+]\s|^\d+[.)]\s|^>\s|^\|/.test(line)
  }
  if (extension === 'json') {
    return /^"[^\"]+"\s*:/.test(line) || /^[{}[\],]/.test(line)
  }
  if (extension === 'toml') {
    return /^\[[^\]]+\]|^[A-Za-z0-9_.-]+\s*=/.test(line)
  }
  return /^[A-Za-z0-9_.-]+\s*:/.test(line) || /^\[[^\]]+\]/.test(line)
}

function formatContext(
  files: WorkingFile[],
  metadata: { generatedAt: string; repoRoot: string; maxTokens: number; compressionLevel: number; fileCount: number },
  outputFormat: 'xml' | 'json',
  deletedFiles: string[]
): string {
  if (outputFormat === 'json') {
    return `${JSON.stringify({
      metadata: {
        generated_at: metadata.generatedAt,
        repo_root: metadata.repoRoot,
        max_tokens: metadata.maxTokens,
        compression_level: metadata.compressionLevel,
        file_count: metadata.fileCount
      },
      project_map: files.map(file => ({
        path: file.path,
        level: file.level,
        tokens: file.tokenCount,
        hash: file.contentHash
      })),
      ...(deletedFiles.length > 0 ? { deleted_files: deletedFiles } : {}),
      files: files.map(file => ({
        path: file.path,
        level: file.level,
        tokens: file.tokenCount,
        content: contentForLevel(file.variants, file.level)
      }))
    }, null, 2)}\n`
  }

  const projectMap = files.map(file => {
    const hash = ` hash="${escapeXml(file.contentHash)}"`
    return `<entry path="${escapeXml(file.path)}" level="${file.level}" tokens="${file.tokenCount}"${hash} />`
  }).join('\n')
  const deleted = deletedFiles.length === 0
    ? ''
    : `\n<deleted_files>\n${deletedFiles.map(file => `<file path="${escapeXml(file)}" />`).join('\n')}\n</deleted_files>`
  const fileContent = files.map(file => {
    const content = escapeXml(contentForLevel(file.variants, file.level))
    return `<file path="${escapeXml(file.path)}" level="${file.level}" tokens="${file.tokenCount}">${content}</file>`
  }).join('\n')

  return `<repository_context>\n<metadata generated_at="${escapeXml(metadata.generatedAt)}" repo_root="${escapeXml(metadata.repoRoot)}" max_tokens="${metadata.maxTokens}" compression_level="${metadata.compressionLevel}" file_count="${metadata.fileCount}" />\n<project_map>\n${projectMap}\n</project_map>${deleted}\n<files>\n${fileContent}\n</files>\n</repository_context>\n`
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;')
}
