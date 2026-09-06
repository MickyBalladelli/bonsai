import * as fs from 'fs/promises'
import * as path from 'path'
import * as vscode from 'vscode'

import {
  DEFAULT_MAX_TOKENS,
  DEFAULT_OUTPUT_FILE,
  BonsaiConfig,
  BonsaiFilePriority,
  buildContextPrompt,
  buildStatusText,
  buildSuccessMessage,
  ProjectMapEntry,
  RunReport
} from './bonsai'
import { generateRepository, removeStaleChunks } from './internalGenerator'
import {
  BONSAI_CONTEXT_TOOL_NAME,
  BonsaiGenerateContextTool
} from './languageModelTool'
import { readProjectConfig } from './projectConfig'

type GeneratedContext = {
  contextText: string
  outputFile: string
  outputFiles: string[]
  projectMap: ProjectMapEntry[]
  repositoryUrl?: string
  report: RunReport
  warnings: string[]
}

type GenerateMode = {
  incremental?: boolean
}

let statusItem: vscode.StatusBarItem | undefined
let outputChannel: vscode.OutputChannel | undefined
let extensionVersion = 'unknown'

export function activate(context: vscode.ExtensionContext) {
  extensionVersion = String(context.extension.packageJSON.version ?? 'unknown')
  statusItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100)
  statusItem.command = 'bonsai.moreActions'
  statusItem.text = `Bonsai v${extensionVersion}: ready`
  statusItem.tooltip = 'Bonsai Context Manager uses its self-contained TypeScript engine'
  statusItem.show()
  outputChannel = vscode.window.createOutputChannel('Bonsai')

  context.subscriptions.push(statusItem, outputChannel)

  if (typeof vscode.lm?.registerTool === 'function') {
    context.subscriptions.push(
      vscode.lm.registerTool(BONSAI_CONTEXT_TOOL_NAME, new BonsaiGenerateContextTool(async (workspacePath, request, filePriorities) => {
        const workspaceRoot = getToolWorkspaceRoot(workspacePath)
        const generated = await generateContext(context, {}, workspaceRoot, request, filePriorities)
        return {
          contextText: generated.contextText,
          outputFiles: generated.outputFiles,
          projectMap: generated.projectMap,
          report: generated.report,
          warnings: generated.warnings
        }
      }))
    )
  }

  registerCommand(context, 'bonsai.generateContext', async () => {
    const generated = await generateContext(context)
    await openContextFile(generated.outputFile)
    showSuccessMessage(generated, 'Context files generated and opened.')
  })

  registerCommand(context, 'bonsai.generateAndAsk', async () => {
    const generated = await generateContext(context)
    const prompt = buildGeneratedPrompt(generated, true)
    await openContextFile(generated.outputFile)
    await openChat(prompt)
    showSuccessMessage(generated, 'Chat opened with instructions to read the generated context files.')
  })

  registerCommand(context, 'bonsai.generateForRequest', async () => {
    const request = await vscode.window.showInputBox({
      prompt: 'What should Bonsai preserve in more detail?',
      placeHolder: 'Example: fix authentication flow or review database migrations'
    })
    if (!request?.trim()) {
      return
    }
    const generated = await generateContext(context, {}, undefined, request.trim())
    await openContextFile(generated.outputFile)
    showSuccessMessage(generated, 'Context files generated with request-aware detail.')
  })

  registerCommand(context, 'bonsai.generateChangedContext', async () => {
    const generated = await generateContext(context, { incremental: true })
    await openContextFile(generated.outputFile)
    showSuccessMessage(generated, 'Changed context files generated and opened.')
  })

  registerCommand(context, 'bonsai.previewProjectMap', async () => {
    const generated = await generateContext(context)
    showProjectMapPreview(context, generated)
    showSuccessMessage(generated, 'Project map preview opened.')
  })

  registerCommand(context, 'bonsai.openContext', async () => {
    const workspaceRoot = await getWorkspaceRoot()
    await openContextFile((await getConfig(workspaceRoot)).outputFile)
  })

  registerCommand(context, 'bonsai.moreActions', async () => {
    await showMoreActions()
  })

  registerCommand(context, 'bonsai.initAgent', async () => {
    await addAgentInstructions()
  })

  registerCommand(context, 'bonsai.createProjectConfig', async () => {
    await createProjectConfig()
  })

}

export function deactivate() {}

function registerCommand(
  context: vscode.ExtensionContext,
  command: string,
  handler: () => Promise<void>
): void {
  const disposable = vscode.commands.registerCommand(command, async () => {
    try {
      await handler()
    } catch (error) {
      await showCommandError(command, error)
    }
  })
  context.subscriptions.push(disposable)
}

async function showMoreActions(): Promise<void> {
  const actions: Array<vscode.QuickPickItem & { command: string }> = [
    {
      label: 'Open Last Context',
      description: 'Open the most recent generated context file',
      command: 'bonsai.openContext'
    }
  ]
  const selected = await vscode.window.showQuickPick(actions, {
    placeHolder: 'Choose a secondary Bonsai action'
  })
  if (selected) {
    await vscode.commands.executeCommand(selected.command)
  }
}

const AGENT_SECTION_START = '<!-- BONSAI_CONTEXT_MANAGER_START -->'
const AGENT_SECTION_END = '<!-- BONSAI_CONTEXT_MANAGER_END -->'

async function addAgentInstructions(): Promise<void> {
  const workspaceRoot = await getWorkspaceRoot()
  const agentPath = path.join(workspaceRoot, 'AGENTS.md')
  let existing = ''
  try {
    existing = await fs.readFile(agentPath, 'utf8')
  } catch {
    existing = ''
  }

  const section = [
    AGENT_SECTION_START,
    '# Mandatory Repository Preflight',
    '',
    'Before answering any broad question, performing a repository-wide review or',
    'making cross-cutting changes, you MUST complete the preflight:',
    '',
    '1. Check the repository root for `bonsai.xml`, `bonsai.json`, or numbered',
    '   files such as `bonsai-2.xml` and `bonsai-2.json`.',
    '2. If any exist, read all of them before continuing.',
    '3. If none exist, invoke the `Generate Bonsai Context` tool',
    '   (`#bonsai_generate_context`) with the user request. Bonsai derives likely',
    '   files itself. If you already know exact files, pass `filePriorities` as',
    '   optional hints: level 1 for primary files, level 2 for support, level 3',
    '   for compact background context.',
    '4. Do not begin the repository-wide task until the context has been read or',
    '   generated.',
    '5. If the generation tool is unavailable, stop and tell the user that Bonsai',
    '   context must be generated or the tool enabled.',
    '',
    'This preflight is required for broad repository tasks. It may be skipped for',
    'narrow questions limited to a named file or symbol.',
    AGENT_SECTION_END
  ].join('\n')
  const sectionPattern = new RegExp(
    `${escapeRegExp(AGENT_SECTION_START)}[\\s\\S]*?${escapeRegExp(AGENT_SECTION_END)}`
  )
  const updated = sectionPattern.test(existing)
    ? existing.replace(sectionPattern, section)
    : `${existing.trimEnd()}${existing.trim() ? '\n\n' : ''}${section}\n`

  await fs.writeFile(agentPath, updated, 'utf8')
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(agentPath), { preview: false })
  vscode.window.showInformationMessage(`Bonsai instructions added to ${agentPath}.`)
}

async function createProjectConfig(): Promise<void> {
  const workspaceRoot = await getWorkspaceRoot()
  const configPath = path.join(workspaceRoot, '.bonsai.toml')
  try {
    await fs.access(configPath)
    await openContextFile(configPath)
    vscode.window.showInformationMessage(`${configPath} already exists.`)
    return
  } catch {
    // Create the file below.
  }

  const contents = [
    '# Bonsai project settings. This file is used by the CLI and VS Code extension.',
    'max_tokens = 12000',
    'tokenizer = "cl100k_base"',
    'level = 1',
    'format = "xml"',
    'output_file = "bonsai.xml"',
    'include = []',
    'exclude = []',
    'respect_gitignore = true',
    ''
  ].join('\n')
  await fs.writeFile(configPath, contents, { encoding: 'utf8', flag: 'wx' })
  await openContextFile(configPath)
  vscode.window.showInformationMessage(`Created ${configPath}.`)
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\\]\\]/g, '\\$&')
}

async function generateContext(
  context: vscode.ExtensionContext,
  mode: GenerateMode = {},
  workspaceRootOverride?: string,
  focus?: string,
  filePriorities?: BonsaiFilePriority[]
): Promise<GeneratedContext> {
  const workspaceRoot = workspaceRootOverride ?? await getWorkspaceRoot()
  const config = await getConfig(workspaceRoot)
  const stateKey = `bonsai.fileSignatures:${workspaceRoot}`
  const previousSignatures = context.workspaceState.get<Record<string, string>>(stateKey)
  const generated = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: mode.incremental ? 'Generating changed Bonsai context' : 'Generating Bonsai context',
      cancellable: false
    },
    () => generateRepository(workspaceRoot, config, {
      incremental: mode.incremental,
      previousSignatures,
      focus,
      filePriorities
    })
  )

  const plannedPaths = [...new Set((filePriorities ?? []).map(priority => normalizePlanPath(priority.path)))]
  const generatedPaths = new Set(generated.projectMap.map(entry => entry.path))
  const missingPaths = plannedPaths.filter(relativePath => !generatedPaths.has(relativePath))
  if (missingPaths.length > 0) {
    throw new Error(`The agent file plan did not match scanned workspace files: ${missingPaths.slice(0, 8).join(', ')}. Inspect the workspace and retry with exact relative paths.`)
  }

  for (const output of generated.contextFiles) {
    await fs.mkdir(path.dirname(output.outputFile), { recursive: true })
    await fs.writeFile(output.outputFile, output.contextText, 'utf8')
  }
  // Regeneration may produce fewer chunks than the previous run. Retire only
  // numbered chunks of this output (base-2.ext, ...); unrelated files are
  // never touched.
  const retired = await removeStaleChunks(
    config.outputFile,
    generated.contextFiles.map(output => output.outputFile)
  )
  for (const stale of retired) {
    appendOutput(`Removed stale context chunk ${stale}`)
  }
  await context.workspaceState.update(stateKey, generated.signatures)
  appendOutput(`Generated ${generated.contextFiles.length} context file${generated.contextFiles.length === 1 ? '' : 's'} with the internal engine (${generated.report.outputTokens ?? generated.report.shrunkTokens ?? '?'} / ${generated.report.outputTokensBudget ?? '?'} tokens)`)
  for (const warning of generated.warnings) {
    appendOutput(`WARNING: ${warning}`)
  }
  if (isOverBudget(generated.report)) {
    appendOutput(`WARNING: Output is ${generated.report.shrunkTokens} tokens, above max_tokens ${generated.report.outputTokensBudget} even after maximum compression; treat this context as incomplete.`)
  }

  const result = {
    contextText: generated.contextFiles[0].contextText,
    outputFile: config.outputFile,
    outputFiles: generated.contextFiles.map(output => output.outputFile),
    projectMap: generated.projectMap,
    repositoryUrl: generated.repositoryUrl,
    report: generated.report,
    warnings: generated.warnings
  }
  updateStatus(result)
  return result
}

function isOverBudget(report: RunReport): boolean {
  return report.shrunkTokens !== undefined
    && report.outputTokensBudget !== undefined
    && report.shrunkTokens > report.outputTokensBudget
}

function normalizePlanPath(value: string): string {
  return path.posix.normalize(value.replace(/\\/g, '/').replace(/^\.\//, ''))
}

function getToolWorkspaceRoot(workspacePath?: string): string {
  const folders = vscode.workspace.workspaceFolders ?? []
  if (folders.length === 0) {
    throw new Error('Open a workspace folder before asking Bonsai to generate context.')
  }

  if (workspacePath) {
    if (!path.isAbsolute(workspacePath)) {
      throw new Error('workspacePath must be an absolute workspace folder path.')
    }
    const requestedPath = path.resolve(workspacePath)
    const matchingFolder = folders.find(folder => {
      const folderPath = path.resolve(folder.uri.fsPath)
      const relativePath = path.relative(folderPath, requestedPath)
      return relativePath === '' || (
        relativePath !== '..' &&
        !relativePath.startsWith(`..${path.sep}`) &&
        !path.isAbsolute(relativePath)
      )
    })
    if (!matchingFolder) {
      throw new Error('workspacePath must point inside an open VS Code workspace folder.')
    }
    return matchingFolder.uri.fsPath
  }

  const activeEditor = vscode.window.activeTextEditor
  const activeFolder = activeEditor
    ? vscode.workspace.getWorkspaceFolder(activeEditor.document.uri)
    : undefined
  return activeFolder?.uri.fsPath ?? folders[0].uri.fsPath
}

async function getWorkspaceRoot(): Promise<string> {
  const folders = vscode.workspace.workspaceFolders ?? []
  if (folders.length === 0) {
    throw new Error('Open a workspace folder before running Bonsai.')
  }

  if (folders.length === 1) {
    return folders[0].uri.fsPath
  }

  const items = folders.map(folder => ({
    label: folder.name,
    description: folder.uri.fsPath,
    folder
  }))
  const selected = await vscode.window.showQuickPick(items, {
    placeHolder: 'Choose the repository for Bonsai'
  })
  if (!selected) {
    throw new Error('Choose a workspace folder before running Bonsai.')
  }

  return selected.folder.uri.fsPath
}

async function getConfig(workspaceRoot: string): Promise<BonsaiConfig> {
  const config = vscode.workspace.getConfiguration('bonsai')
  const projectConfig = await readProjectConfig(workspaceRoot)
  const configuredOutputFile = projectConfig.outputFile ?? config.get<string>('outputFile', DEFAULT_OUTPUT_FILE)
  return {
    exclude: projectConfig.exclude ?? config.get<string[]>('exclude', []),
    include: projectConfig.include ?? config.get<string[]>('include', []),
    level: projectConfig.level ?? config.get<number>('level', 1),
    maxTokens: projectConfig.maxTokens ?? config.get<number>('maxTokens', DEFAULT_MAX_TOKENS),
    outputFile: path.isAbsolute(configuredOutputFile)
      ? configuredOutputFile
      : path.join(workspaceRoot, configuredOutputFile),
    outputFormat: projectConfig.outputFormat ?? config.get<'json' | 'xml'>('outputFormat', 'xml'),
    respectGitignore: projectConfig.respectGitignore ?? config.get<boolean>('respectGitignore', true)
  }
}

async function showCommandError(command: string, error: unknown): Promise<void> {
  const message = error instanceof Error ? error.message : String(error)
  appendOutput(`ERROR in ${command}: ${message}`)
  setHealthStatus('error', `Bonsai extension v${extensionVersion}\n${message}`)
  const choice = await vscode.window.showErrorMessage(
    `Bonsai failed: ${message}`,
    'Show Bonsai Output'
  )
  if (choice === 'Show Bonsai Output') {
    outputChannel?.show(true)
  }
}

async function openContextFile(outputFile: string): Promise<void> {
  const document = await vscode.workspace.openTextDocument(vscode.Uri.file(outputFile))
  await vscode.window.showTextDocument(document, { preview: false })
}

async function openChat(prompt: string): Promise<void> {
  try {
    await vscode.commands.executeCommand('workbench.action.chat.open', prompt)
  } catch {
    await vscode.commands.executeCommand('workbench.action.chat.open')
  }
}

function showSuccessMessage(generated: GeneratedContext, nextStep: string): void {
  if (generated.outputFiles.length > 1) {
    nextStep = `${nextStep} Output split across ${generated.outputFiles.length} files under 10 MB each.`
  }
  const message = buildSuccessMessage(generated.outputFile, generated.report, nextStep)
  if (isOverBudget(generated.report)) {
    void vscode.window.showWarningMessage(message, 'Show Bonsai Output').then(choice => {
      if (choice === 'Show Bonsai Output') {
        outputChannel?.show(true)
      }
    })
    return
  }
  vscode.window.showInformationMessage(message)
}

function buildGeneratedPrompt(generated: GeneratedContext, includeContent = false): string {
  const prompt = generated.outputFiles.length === 1 && includeContent
    ? buildContextPrompt(generated.outputFile, generated.contextText)
    : buildContextPrompt(generated.outputFile)
  if (generated.outputFiles.length === 1) {
    return prompt
  }

  return `${prompt}\n\nRead these additional Bonsai context files too before answering:\n${generated.outputFiles.slice(1).join('\n')}`
}

function setHealthStatus(state: string, tooltip: string): void {
  if (!statusItem) {
    return
  }
  statusItem.text = `Bonsai v${extensionVersion}: ${state}`
  statusItem.tooltip = tooltip
  statusItem.show()
}

function updateStatus(generated: GeneratedContext): void {
  if (!statusItem) {
    return
  }

  const reportText = buildStatusText(generated.report).replace(/^Bonsai:\s*/, '')
  statusItem.text = `Bonsai v${extensionVersion}: ${reportText}`
  statusItem.tooltip = [
    `Bonsai extension v${extensionVersion}`,
    'Engine: self-contained TypeScript',
    `Output: ${generated.outputFile}`
  ].join('\n')
  statusItem.show()
}

function appendOutput(value: string): void {
  const text = value.trim()
  if (text) {
    outputChannel?.appendLine(text)
  }
}

function showProjectMapPreview(context: vscode.ExtensionContext, generated: GeneratedContext): void {
  const panel = vscode.window.createWebviewPanel(
    'bonsaiProjectMap',
    'Bonsai Project Map',
    vscode.ViewColumn.Beside,
    { enableScripts: false }
  )
  panel.webview.html = buildProjectMapHtml(generated)
  context.subscriptions.push(panel)
}

function buildProjectMapHtml(generated: GeneratedContext): string {
  const rows = generated.projectMap
    .map(entry => `<tr><td>${escapeHtml(entry.path)}</td><td>${entry.level}</td><td>${entry.tokens}</td><td>${formatSavedPercent(entry.savedPercent)}</td><td>${escapeHtml(entry.reason ?? 'background module')}</td></tr>`)
    .join('')
  const repositoryLink = generated.repositoryUrl
    ? `<p>Repository: <a href="${escapeHtml(generated.repositoryUrl)}" target="_blank" rel="noopener">${escapeHtml(generated.repositoryUrl)}</a></p>`
    : ''

  return `<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, sans-serif; padding: 16px; }
    table { border-collapse: collapse; width: 100%; }
    th, td { border-bottom: 1px solid #ddd; padding: 6px 8px; text-align: left; }
    th { position: sticky; top: 0; background: var(--vscode-editor-background); }
    td:nth-child(2), td:nth-child(3), td:nth-child(4) { text-align: right; white-space: nowrap; }
  </style>
</head>
<body>
  <h1>Bonsai Project Map</h1>
  <p>${escapeHtml(generated.outputFile)}</p>
  ${repositoryLink}
  <p>Saved = original source tokens versus compressed output tokens.</p>
  <table>
    <thead><tr><th>Path</th><th>Level</th><th>Tokens</th><th>Saved</th><th>Why</th></tr></thead>
    <tbody>${rows}</tbody>
  </table>
</body>
</html>`
}

function formatSavedPercent(value: number | undefined): string {
  return value === undefined ? '—' : `${value.toFixed(2)}%`
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}
