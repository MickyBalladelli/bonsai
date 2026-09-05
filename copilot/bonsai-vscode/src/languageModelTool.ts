import * as vscode from 'vscode'

import { BonsaiFilePriority, buildProjectMapDecisionLines, ProjectMapEntry, RunReport } from './bonsai'

export const BONSAI_CONTEXT_TOOL_NAME = 'bonsai_generate_context'

export type BonsaiToolInput = {
  workspacePath?: string
  request: string
  filePriorities?: BonsaiFilePriority[]
}

export type BonsaiToolGeneration = {
  contextText: string
  outputFiles: string[]
  projectMap: ProjectMapEntry[]
  report: RunReport
  warnings: string[]
}

export type BonsaiToolGenerator = (workspacePath?: string, request?: string, filePriorities?: BonsaiFilePriority[]) => Promise<BonsaiToolGeneration>

export class BonsaiGenerateContextTool implements vscode.LanguageModelTool<BonsaiToolInput> {
  constructor(private readonly generate: BonsaiToolGenerator) {}

  prepareInvocation(
    options: vscode.LanguageModelToolInvocationPrepareOptions<BonsaiToolInput>,
    _token: vscode.CancellationToken
  ): vscode.PreparedToolInvocation {
    const location = options.input.workspacePath
      ? ` at ${options.input.workspacePath}`
      : ' in the active workspace'
    return {
      invocationMessage: 'Generating Bonsai repository context',
      confirmationMessages: {
        title: 'Generate Bonsai context files',
        message: `Generate or update Bonsai context files${location}?`
      }
    }
  }

  async invoke(
    options: vscode.LanguageModelToolInvocationOptions<BonsaiToolInput>,
    token: vscode.CancellationToken
  ): Promise<vscode.LanguageModelToolResult> {
    if (token.isCancellationRequested) {
      throw new Error('Bonsai context generation was cancelled.')
    }

    const request = options.input.request?.trim()
    if (!request) {
      throw new Error('A repository question or task is required for task-aware Bonsai generation.')
    }
    const filePriorities = options.input.filePriorities ?? []
    if (!Array.isArray(filePriorities)) {
      throw new Error('filePriorities must be an array when provided.')
    }
    if (filePriorities.length > 0) {
      const invalidPriority = filePriorities.find(priority =>
        !priority || typeof priority.path !== 'string' || !priority.path.trim() ||
        !Number.isInteger(priority.level) || priority.level < 1 || priority.level > 3 ||
        (priority.includeComments !== undefined && typeof priority.includeComments !== 'boolean')
      )
      if (invalidPriority) {
        throw new Error('Every file priority needs a workspace-relative path and an integer level from 1 to 3.')
      }
    }
    const generated = await this.generate(options.input.workspacePath, request, filePriorities)
    if (token.isCancellationRequested) {
      throw new Error('Bonsai context generation was cancelled.')
    }

    return new vscode.LanguageModelToolResult([
      new vscode.LanguageModelTextPart(buildToolResult(generated))
    ])
  }
}

export function buildToolResult(generated: BonsaiToolGeneration): string {
  const files = generated.outputFiles.map(file => `- ${file}`).join('\n')
  const decisions = buildProjectMapDecisionLines(generated.projectMap)
  const saved = generated.report.savingPercent === undefined
    ? 'unknown'
    : `${generated.report.savingPercent.toFixed(2)}%`
  const contextLimit = 200_000
  const context = generated.contextText.length > contextLimit
    ? `${generated.contextText.slice(0, contextLimit)}\n...[first context file truncated; read the file for the complete contents]`
    : generated.contextText

  return [
    'Bonsai generated repository context successfully.',
    `Overall compression saved ${saved} of tokens.`,
    'The agent priority plan was applied when provided. Level 1 preserves full source including implementation logic, types, configuration values, and meaningful comments, and fails if it cannot fit the token budget instead of silently downgrading or truncating; levels 2 and 3 are explicit opt-in lossy compression. Unlisted files are compressed harder first only in explicit lossy levels.',
    ...(generated.warnings.length > 0
      ? [
        'Fidelity warnings: this context is lossy and must not be treated as complete evidence for a full review.',
        ...generated.warnings.map(warning => `- ${warning}`)
      ]
      : []),
    'Project map detail decisions:',
    decisions.join('\n') || '- Full project map is in the generated context file.',
    'Context files written:',
    files,
    '',
    'Use these files as the repository source of truth. The first context file contents follow:',
    context
  ].join('\n')
}
