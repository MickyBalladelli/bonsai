import * as vscode from 'vscode'

import { RunReport } from './bonsai'

export const BONSAI_CONTEXT_TOOL_NAME = 'bonsai_generate_context'

export type BonsaiToolInput = {
  workspacePath?: string
  request?: string
}

export type BonsaiToolGeneration = {
  contextText: string
  outputFiles: string[]
  report: RunReport
}

export type BonsaiToolGenerator = (workspacePath?: string, request?: string) => Promise<BonsaiToolGeneration>

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

    const generated = await this.generate(options.input.workspacePath, options.input.request)
    if (token.isCancellationRequested) {
      throw new Error('Bonsai context generation was cancelled.')
    }

    return new vscode.LanguageModelToolResult([
      new vscode.LanguageModelTextPart(buildToolResult(generated))
    ])
  }
}

function buildToolResult(generated: BonsaiToolGeneration): string {
  const files = generated.outputFiles.map(file => `- ${file}`).join('\n')
  const saved = generated.report.savingPercent === undefined
    ? 'unknown'
    : `${generated.report.savingPercent.toFixed(2)}%`
  const contextLimit = 200_000
  const context = generated.contextText.length > contextLimit
    ? `${generated.contextText.slice(0, contextLimit)}\n...[first context file truncated; read the file for the complete contents]`
    : generated.contextText

  return [
    'Bonsai generated repository context successfully.',
    `Overall estimated compression saved ${saved} of tokens.`,
    'Context files written:',
    files,
    '',
    'Use these files as the repository source of truth. The first context file contents follow:',
    context
  ].join('\n')
}
